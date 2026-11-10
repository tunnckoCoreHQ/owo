//! `owo sync start` / `owo sync done`: host-agnostic session sync.
//! The host calls `start` after each turn; when enough new user turns exist, owo writes the
//! slice to `<root>/sources/<slug>/` and an ingest prompt to `<root>/.owo/sessions/`.
//! The host runs the ingest agent and then calls `done --until <n>` to move the bookmark.

use crate::identity::{read_json, write_json};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const FORMATS: &[&str] = &["jsonl", "claude-jsonl", "opencode-json"];
const OVERLAP_TURNS: usize = 2;

#[derive(Debug, Clone)]
pub struct Message {
    /// The host's id for the message, when the transcript has one.
    pub id: Option<String>,
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

/// A string or number id as text.
fn id_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Parses a transcript into user and assistant messages with text.
pub fn parse(raw: &str, format: &str) -> Result<Vec<Message>> {
    let mut out = vec![];
    let mut push = |id: Option<String>, role: Option<&str>, text: String| {
        if let Some(role @ ("user" | "assistant")) = role {
            if !text.trim().is_empty() {
                out.push(Message {
                    id,
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
                        id_of(&v["id"]),
                        v["role"].as_str(),
                        v["text"].as_str().unwrap_or_default().to_string(),
                    );
                } else {
                    let msg = &v["message"];
                    let role = msg["role"].as_str().or(v["type"].as_str());
                    push(id_of(&v["uuid"]), role, text_of(&msg["content"]));
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
                let id = id_of(&m["id"]).or_else(|| m.pointer("/info/id").and_then(id_of));
                push(id, role, text);
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
    /// The prompt of a run that started earlier and has no `sync done` yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<String>,
    /// Push only: turns stored by this push.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<usize>,
    /// Push only, when due: the slice and prompt text, for agents that cannot read the files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_text: Option<String>,
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

/// Holds `<root>/.owo/sessions/<id>.lock` while one sync step reads and writes the bookmark,
/// so parallel calls for one session run one after the other.
struct Lock(PathBuf);

impl Lock {
    fn take(root: &Path, session: &str) -> Result<Lock> {
        let path = root.join(".owo/sessions").join(format!("{session}.lock"));
        fs::create_dir_all(path.parent().expect("has parent"))?;
        for _ in 0..100 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Lock(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // A lock older than a minute belongs to a process that ended without cleanup.
                    let stale = fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .is_ok_and(|t| t.elapsed().unwrap_or_default() > Duration::from_secs(60));
                    if stale {
                        let _ = fs::remove_file(&path);
                    } else {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        bail!(
            "session {session:?} is busy: another `owo sync` holds {}",
            path.display()
        )
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn bookmark_path(root: &Path, session: &str) -> std::path::PathBuf {
    root.join(".owo/sessions").join(format!("{session}.json"))
}

fn read_bookmark(root: &Path, session: &str) -> Value {
    read_json(&bookmark_path(root, session)).unwrap_or_else(|| json!({}))
}

fn write_bookmark(root: &Path, session: &str, doc: &Value) -> Result<()> {
    let path = bookmark_path(root, session);
    fs::create_dir_all(path.parent().expect("has parent"))?;
    write_json(&path, doc)
}

pub fn start(req: &StartRequest) -> Result<Start> {
    check_session(req.session)?;
    let _lock = Lock::take(req.root, req.session)?;
    run(req)
}

/// Stores pushed `{id, role, text}` turns in `<root>/.owo/sessions/<id>.turns.jsonl`,
/// skipping ids already stored, then runs `start` on that file.
pub fn push(req: &StartRequest, turns: &str) -> Result<Start> {
    check_session(req.session)?;
    let _lock = Lock::take(req.root, req.session)?;
    let name = format!("{}.turns.jsonl", req.session);
    let file = req.root.join(".owo/sessions").join(&name);
    if let Some(t) = read_bookmark(req.root, req.session)["transcript"].as_str() {
        if !t.ends_with(&format!(".owo/sessions/{name}")) {
            bail!(
                "session {:?} syncs the transcript {t}; use `owo sync start`",
                req.session
            );
        }
    }
    let list: Value = serde_json::from_str(turns).context("turns are not JSON")?;
    let list = list
        .as_array()
        .context("turns must be a JSON array of {id, role, text}")?;
    let mut turns = vec![];
    for (n, t) in list.iter().enumerate() {
        let id =
            id_of(&t["id"]).with_context(|| format!("turn {n}: id must be a string or number"))?;
        let role = t["role"]
            .as_str()
            .filter(|r| matches!(*r, "user" | "assistant"))
            .with_context(|| format!("turn {n}: role must be user or assistant"))?;
        let text = t["text"]
            .as_str()
            .with_context(|| format!("turn {n}: text must be a string"))?;
        if !text.trim().is_empty() {
            turns.push(json!({ "id": id, "role": role, "text": text }));
        }
    }
    let stored = fs::read_to_string(&file).unwrap_or_default();
    let mut ids: HashSet<String> = parse(&stored, "jsonl")?
        .into_iter()
        .filter_map(|m| m.id)
        .collect();
    let mut lines = String::new();
    for t in turns {
        if ids.insert(t["id"].as_str().expect("id is text").to_string()) {
            lines.push_str(&format!("{t}\n"));
        }
    }
    let added = lines.lines().count();
    fs::create_dir_all(file.parent().expect("has parent"))?;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)?
        .write_all(lines.as_bytes())?;
    let mut out = run(&StartRequest {
        transcript: &file,
        format: "jsonl",
        ..*req
    })?;
    out.added = Some(added);
    if let (Some(source), Some(prompt)) = (&out.source, &out.prompt) {
        out.source_text = Some(fs::read_to_string(req.root.join(source))?);
        out.prompt_text = Some(fs::read_to_string(req.root.join(prompt))?);
    }
    Ok(out)
}

/// Reads the transcript, decides whether a run is due and writes the slice and prompt.
/// The caller holds the session lock.
fn run(req: &StartRequest) -> Result<Start> {
    let raw = fs::read_to_string(req.transcript)
        .with_context(|| format!("reading {}", req.transcript.display()))?;
    let messages = parse(&raw, req.format)?;
    if messages.is_empty() && !raw.trim().is_empty() {
        bail!(
            "no user or assistant messages in {} as {}; check --format",
            req.transcript.display(),
            req.format
        );
    }
    let mut doc = read_bookmark(req.root, req.session);
    let done = doc["until"].as_u64().unwrap_or(0) as usize;
    let done = done.min(messages.len());
    let until = messages.len();
    // While a run is pending, count from its end so the next turns do not start it again.
    // The slice still starts at the bookmark, so a run that never finished is covered again.
    let counted_from = doc["pending"]["until"]
        .as_u64()
        .map_or(done, |u| (u as usize).clamp(done, until));
    let new_user = messages[counted_from..]
        .iter()
        .filter(|m| m.role == "user")
        .count();
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
        pending: None,
        added: None,
        source_text: None,
        prompt_text: None,
    };
    doc["session"] = json!(req.session);
    doc["identity"] = json!(req.slug);
    doc["transcript"] = json!(req.transcript.display().to_string());
    doc["format"] = json!(req.format);
    doc["messages"] = json!(until);
    if doc.get("until").is_none() {
        doc["until"] = json!(0);
    }
    if !due {
        out.pending = doc["pending"]["prompt"].as_str().map(str::to_string);
    }
    if due {
        let date = crate::state::now()[..10].to_string();
        let source = format!(
            "sources/{}/{date}_{}_{from}-{until}.md",
            req.slug, req.session
        );
        let mut body = format!("# Session {} ({from}-{until})\n\nIdentity: {}. Transcript: {}. Messages {from} to {until}.{}\n", req.session, req.slug, req.transcript.display(), if done == 0 { String::new() } else { format!(" Messages before {done} were synced already.") });
        for (i, m) in messages[from..until].iter().enumerate() {
            let id =
                m.id.as_ref()
                    .map(|id| format!(" ({id})"))
                    .unwrap_or_default();
            body.push_str(&format!("\n## {} {}{id}\n\n{}\n", from + i, m.role, m.text));
        }
        let path = req.root.join(&source);
        fs::create_dir_all(path.parent().expect("has parent"))?;
        fs::write(&path, body)?;
        let prompt = format!(".owo/sessions/{}.prompt.md", req.session);
        let project = req
            .project
            .map(|p| format!(" --project {p}"))
            .unwrap_or_default();
        let root = fs::canonicalize(req.root).unwrap_or_else(|_| req.root.to_path_buf());
        let quoted = format!("'{}'", root.display().to_string().replace('\'', "'\\''"));
        let text = format!(
            "Use the identity-ingest skill for identity `{slug}`{proj}.\n\nSource: `{source}` (relative to the identities root `{root}`).{earlier}\n\n1. `{owo} context --format text --no-touch{project}` and `{owo} state`.\n2. Route each durable fact in the source to its home; add `{cite}#L<a>-L<b>` to each entry's frontmatter `sources`.\n3. Finish with `{owo} stamp && {owo} index && {owo} lint --changed`.\n4. Report changed paths or \"no changes\".\n\nWhen it succeeds the host runs `owo --root {quoted} sync done --session {session} --until {until}`.\n",
            slug = req.slug,
            earlier = if done == 0 { String::new() } else { format!(" Messages before {done} were ingested in an earlier run; the first part is overlap for context.") },
            root = root.display(),
            owo = format!("owo --root {quoted} -i {}", req.slug),
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
    let _lock = Lock::take(root, session)?;
    let mut doc = read_bookmark(root, session);
    if doc.get("session").is_none() {
        bail!("no sync bookmark for session {session:?}; run `owo sync start` first");
    }
    let prev = doc["until"].as_u64().unwrap_or(0) as usize;
    if until < prev {
        bail!("--until {until} is behind the bookmark ({prev})");
    }
    if let Some(seen) = doc["messages"].as_u64().filter(|n| until as u64 > *n) {
        bail!("--until {until} is past the {seen} messages the last `sync start` read");
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
