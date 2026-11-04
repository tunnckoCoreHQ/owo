//! `owo sync start` / `owo sync done`: host-agnostic session sync.
//! The host calls `start` after each turn; when enough new user turns exist, owo writes the
//! slice to `<root>/sources/<slug>/` and an ingest prompt to `<root>/.owo/sessions/`.
//! The host runs the ingest agent and then calls `done --until <n>` to move the bookmark.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub const FORMATS: &[&str] = &["jsonl", "claude-jsonl", "opencode-json"];
const OVERLAP_TURNS: usize = 2;

#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub text: String,
}

fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|p| {
                p.get("type")
                    .and_then(Value::as_str)
                    .is_none_or(|t| t == "text")
            })
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Parses a transcript into user and assistant messages with text.
pub fn parse(raw: &str, format: &str) -> Result<Vec<Message>> {
    let mut out = vec![];
    let mut push = |role: Option<&str>, text: String| {
        if let Some(role @ ("user" | "assistant")) = role {
            if !text.trim().is_empty() {
                out.push(Message {
                    role: role.to_string(),
                    text: text.trim().to_string(),
                });
            }
        }
    };
    match format {
        "jsonl" | "claude-jsonl" => {
            for (n, line) in raw
                .lines()
                .enumerate()
                .filter(|(_, l)| !l.trim().is_empty())
            {
                let v: Value = serde_json::from_str(line)
                    .with_context(|| format!("transcript line {}", n + 1))?;
                if format == "jsonl" {
                    push(
                        v["role"].as_str(),
                        v["text"].as_str().unwrap_or_default().to_string(),
                    );
                } else {
                    let msg = &v["message"];
                    let role = msg["role"].as_str().or(v["type"].as_str());
                    push(role, text_of(&msg["content"]));
                }
            }
        }
        "opencode-json" => {
            let v: Value = serde_json::from_str(raw).context("transcript is not JSON")?;
            let list = v.get("messages").cloned().unwrap_or(v);
            for m in list.as_array().context("expected an array of messages")? {
                let role = m["role"]
                    .as_str()
                    .or(m.pointer("/info/role").and_then(Value::as_str));
                let text = if m.get("parts").is_some() {
                    text_of(&m["parts"])
                } else {
                    text_of(&m["content"])
                };
                push(role, text);
            }
        }
        other => bail!(
            "unknown transcript format {other:?} ({})",
            FORMATS.join(", ")
        ),
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct Start {
    pub due: bool,
    pub identity: String,
    pub session: String,
    pub new_user_turns: usize,
    pub every: usize,
    pub from: usize,
    pub until: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

pub struct StartRequest<'a> {
    pub root: &'a Path,
    pub slug: &'a str,
    pub session: &'a str,
    pub transcript: &'a Path,
    pub format: &'a str,
    pub now: bool,
    pub every: usize,
    pub project: Option<&'a str>,
}

fn check_session(session: &str) -> Result<()> {
    if session.is_empty()
        || !session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        || session.starts_with('.')
    {
        bail!("session id {session:?} must be letters, digits, '-', '_' or '.'");
    }
    Ok(())
}

fn bookmark_path(root: &Path, session: &str) -> std::path::PathBuf {
    root.join(".owo/sessions").join(format!("{session}.json"))
}

fn read_bookmark(root: &Path, session: &str) -> Value {
    fs::read_to_string(bookmark_path(root, session))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}))
}

fn write_bookmark(root: &Path, session: &str, doc: &Value) -> Result<()> {
    let path = bookmark_path(root, session);
    fs::create_dir_all(path.parent().expect("has parent"))?;
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(doc)?))?;
    Ok(())
}

pub fn start(req: &StartRequest) -> Result<Start> {
    check_session(req.session)?;
    let raw = fs::read_to_string(req.transcript)
        .with_context(|| format!("reading {}", req.transcript.display()))?;
    let messages = parse(&raw, req.format)?;
    let mut doc = read_bookmark(req.root, req.session);
    let done = doc["until"].as_u64().unwrap_or(0) as usize;
    let done = done.min(messages.len());
    let until = messages.len();
    let new_user = messages[done..].iter().filter(|m| m.role == "user").count();
    let due = new_user > 0 && (req.now || new_user >= req.every);
    let mut from = done;
    let mut seen = 0;
    while from > 0 && seen < OVERLAP_TURNS {
        from -= 1;
        if messages[from].role == "user" {
            seen += 1;
        }
    }
    let mut out = Start {
        due,
        identity: req.slug.into(),
        session: req.session.into(),
        new_user_turns: new_user,
        every: req.every,
        from,
        until,
        source: None,
        prompt: None,
    };
    doc["session"] = json!(req.session);
    doc["identity"] = json!(req.slug);
    doc["transcript"] = json!(req.transcript.display().to_string());
    doc["format"] = json!(req.format);
    if doc.get("until").is_none() {
        doc["until"] = json!(0);
    }
    if due {
        let date = crate::state::now()[..10].to_string();
        let source = format!(
            "sources/{}/{date}_{}_{from}-{until}.md",
            req.slug, req.session
        );
        let mut body = format!("# Session {} ({from}-{until})\n\nIdentity: {}. Transcript: {}. Messages {from} to {until}; messages before {done} were synced already.\n", req.session, req.slug, req.transcript.display());
        for (i, m) in messages[from..until].iter().enumerate() {
            body.push_str(&format!("\n## {} {}\n\n{}\n", from + i, m.role, m.text));
        }
        let path = req.root.join(&source);
        fs::create_dir_all(path.parent().expect("has parent"))?;
        fs::write(&path, body)?;
        let prompt = format!(".owo/sessions/{}.prompt.md", req.session);
        let project = req
            .project
            .map(|p| format!(" --project {p}"))
            .unwrap_or_default();
        let text = format!(
            "Use the identity-ingest skill for identity `{slug}`{proj}.\n\nSource: `{source}` (relative to the identities root). Messages before {done} were ingested in an earlier run; the first part is overlap for context.\n\n1. `owo -i {slug} context --format text{project}` and `owo -i {slug} state`.\n2. Route each durable fact in the source to its home; cite `{cite}#L<a>-L<b>` under `## Sources`.\n3. Finish with `owo -i {slug} stamp && owo -i {slug} index && owo -i {slug} lint --changed`.\n4. Report changed paths or \"no changes\".\n\nWhen it succeeds the host runs `owo sync done --session {session} --until {until}`.\n",
            slug = req.slug,
            proj = req.project.map(|p| format!(" on project `{p}`")).unwrap_or_default(),
            cite = source.trim_start_matches("sources/"),
            session = req.session,
        );
        fs::create_dir_all(req.root.join(".owo/sessions"))?;
        fs::write(req.root.join(&prompt), text)?;
        doc["pending"] =
            json!({ "from": from, "until": until, "source": source, "prompt": prompt });
        out.source = Some(source);
        out.prompt = Some(prompt);
    }
    write_bookmark(req.root, req.session, &doc)?;
    Ok(out)
}

pub fn done(root: &Path, session: &str, until: usize) -> Result<Value> {
    check_session(session)?;
    let mut doc = read_bookmark(root, session);
    if doc.get("session").is_none() {
        bail!("no sync bookmark for session {session:?}; run `owo sync start` first");
    }
    let prev = doc["until"].as_u64().unwrap_or(0) as usize;
    if until < prev {
        bail!("--until {until} is behind the bookmark ({prev})");
    }
    doc["until"] = json!(until);
    doc["synced_at"] = json!(crate::state::now());
    if doc
        .get("pending")
        .is_some_and(|p| p["until"].as_u64().is_some_and(|u| u as usize <= until))
    {
        doc.as_object_mut().expect("object").remove("pending");
    }
    write_bookmark(root, session, &doc)?;
    Ok(doc)
}
