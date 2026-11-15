//! `owo sync start` / `owo sync done`: host-agnostic session sync.
//! The host calls `start` after each turn; when enough new user turns exist, owo writes the
//! slice to `<root>/sources/<slug>/` and an ingest prompt to `<root>/.owo/sessions/`.
//! The host runs the ingest agent, whose last step is `done --until <n>`, which moves the bookmark.

use crate::identity::read_object;
use crate::write::Writes;
use crate::Fault;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The format of a host transcript; `Jsonl` by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// One `{"role", "text"}` object per line, with an optional turn `id`.
    #[default]
    Jsonl,
    ClaudeJsonl,
    OpencodeJson,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Jsonl, Format::ClaudeJsonl, Format::OpencodeJson];

    pub fn name(self) -> &'static str {
        match self {
            Format::Jsonl => "jsonl",
            Format::ClaudeJsonl => "claude-jsonl",
            Format::OpencodeJson => "opencode-json",
        }
    }

    pub fn from_name(name: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.name() == name)
    }
}

/// The bookmark of a session: where its transcript is, how far ingest got, and the run that
/// waits for `Root::sync_done`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub session: String,
    pub transcript: String,
    /// The messages the last sync step read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<usize>,
    /// The message index ingest reached.
    pub until: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingRun>,
}

/// A run whose slice and prompt are written, and that has no `Root::sync_done` yet.
#[derive(Debug, Serialize, Deserialize)]
pub struct PendingRun {
    pub from: usize,
    pub until: usize,
    pub source: String,
    pub prompt: String,
}

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
pub fn parse(raw: &str, format: Format) -> Result<Vec<Message>> {
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
    if format == Format::OpencodeJson {
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
        return Ok(out);
    }
    for (n, line) in raw
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let v: Value =
            serde_json::from_str(line).with_context(|| format!("transcript line {}", n + 1))?;
        if format == Format::Jsonl {
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
    /// The caller's folder, which a relative transcript path joins onto.
    pub cwd: &'a Path,
    pub slug: &'a str,
    pub session: &'a str,
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
/// so parallel calls for one session run one after the other. The folders it made go when the
/// lock goes and they are empty, so a failed step leaves none.
struct Lock {
    path: PathBuf,
    _made: Made,
}

/// Folders a lock made, deepest first, removed when they are empty again.
struct Made(Vec<PathBuf>);

impl Drop for Made {
    fn drop(&mut self) {
        for dir in &self.0 {
            let _ = fs::remove_dir(dir);
        }
    }
}

impl Lock {
    /// None in a dry run, which writes no bookmark to guard.
    fn take(root: &Path, session: &str) -> Result<Option<Lock>> {
        if crate::write::dry() {
            return Ok(None);
        }
        let path = root.join(".owo/sessions").join(format!("{session}.lock"));
        let dir = path.parent().expect("has parent");
        let made = Made(crate::write::missing(dir));
        fs::create_dir_all(dir)?;
        for _ in 0..100 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Some(Lock { path, _made: made })),
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
        Err(Fault::SessionBusy {
            session: session.into(),
            lock: path,
        }
        .into())
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn bookmark_path(root: &Path, session: &str) -> std::path::PathBuf {
    root.join(".owo/sessions").join(format!("{session}.json"))
}

fn read_bookmark(root: &Path, session: &str) -> Result<Value> {
    Ok(read_object(&bookmark_path(root, session))?.unwrap_or_else(|| json!({})))
}

fn bookmark_writes(root: &Path, session: &str, doc: &Value) -> Result<Writes> {
    let path = bookmark_path(root, session);
    Writes::new().json(&path, &path, doc)
}

/// The file that `push` stores turns in, relative to the root.
fn store(session: &str) -> String {
    format!(".owo/sessions/{session}.turns.jsonl")
}

pub fn start(req: &StartRequest, transcript: &Path, format: Format) -> Result<Start> {
    check_session(req.session)?;
    let _lock = Lock::take(req.root, req.session)?;
    if let Some(t) = read_bookmark(req.root, req.session)?["transcript"].as_str() {
        if t.ends_with(&store(req.session)) {
            return Err(Fault::PushedSession(req.session.into()).into());
        }
    }
    let raw = fs::read_to_string(req.cwd.join(transcript))
        .with_context(|| format!("reading {}", transcript.display()))?;
    let (out, writes) = run(req, transcript, &raw, format)?;
    writes.commit()?;
    Ok(out)
}

/// One turn a host pushes: its id (the host's id for the message), role and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub id: String,
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// A turn id is not empty and has no control characters.
fn check_turn_id(n: usize, id: &str) -> Result<()> {
    if id.is_empty() || id.chars().any(char::is_control) {
        bail!("turn {n}: the turn id must be a number or a string with no control characters");
    }
    Ok(())
}

/// Parses a JSON array of `{id, role, text}` turns; an id is a string or a number.
pub fn parse_turns(json: &str) -> Result<Vec<Turn>> {
    let list: Value = serde_json::from_str(json).context("turns are not JSON")?;
    let list = list
        .as_array()
        .context("turns must be a JSON array of {id, role, text}")?;
    let mut turns = vec![];
    for (n, t) in list.iter().enumerate() {
        let id = id_of(&t["id"]).unwrap_or_default();
        check_turn_id(n, &id)?;
        let role = match t["role"].as_str() {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            _ => bail!("turn {n}: role must be user or assistant"),
        };
        let text = t["text"]
            .as_str()
            .with_context(|| format!("turn {n}: text must be a string"))?;
        turns.push(Turn {
            id,
            role,
            text: text.to_string(),
        });
    }
    Ok(turns)
}

/// Stores pushed turns in `<root>/.owo/sessions/<id>.turns.jsonl`, skipping ids already
/// stored and turns with no text, then runs `start` on that file.
pub fn push(req: &StartRequest, turns: &[Turn]) -> Result<Start> {
    check_session(req.session)?;
    let _lock = Lock::take(req.root, req.session)?;
    let file = req.root.join(store(req.session));
    if let Some(t) = read_bookmark(req.root, req.session)?["transcript"].as_str() {
        if !t.ends_with(&store(req.session)) {
            return Err(Fault::TranscriptSession {
                session: req.session.into(),
                transcript: t.into(),
            }
            .into());
        }
    }
    for (n, t) in turns.iter().enumerate() {
        check_turn_id(n, &t.id)?;
    }
    let mut bytes = match crate::write::read(&file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => return Err(e).with_context(|| format!("reading {}", file.display())),
    };
    // A write cut off before its newline leaves a partial last line, perhaps mid-character;
    // drop it before decoding.
    bytes.truncate(bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1));
    let mut stored = String::from_utf8(bytes)
        .with_context(|| format!("{} is not UTF-8; fix it by hand", file.display()))?;
    let mut ids: HashSet<String> = parse(&stored, Format::Jsonl)?
        .into_iter()
        .filter_map(|m| m.id)
        .collect();
    let mut added = 0;
    for t in turns.iter().filter(|t| !t.text.trim().is_empty()) {
        if ids.insert(t.id.clone()) {
            let line = json!({ "id": t.id, "role": t.role.name(), "text": t.text });
            stored.push_str(&format!("{line}\n"));
            added += 1;
        }
    }
    let (mut out, writes) = run(req, &file, &stored, Format::Jsonl)?;
    Writes::new()
        .file(&file, &file, stored)
        .and(writes)
        .commit()?;
    out.added = Some(added);
    if let (Some(source), Some(prompt)) = (&out.source, &out.prompt) {
        out.source_text = Some(crate::write::read_to_string(&req.root.join(source))?);
        out.prompt_text = Some(crate::write::read_to_string(&req.root.join(prompt))?);
    }
    Ok(out)
}

/// Parses `raw`, the text of `transcript`, decides whether a run is due, and returns the writes
/// of the slice, the prompt and the bookmark for the caller to commit. The caller holds the lock.
fn run(
    req: &StartRequest,
    transcript: &Path,
    raw: &str,
    format: Format,
) -> Result<(Start, Writes)> {
    let messages = parse(raw, format)?;
    if messages.is_empty() && !raw.trim().is_empty() {
        return Err(Fault::NoMessages {
            transcript: transcript.to_path_buf(),
            format: format.name(),
        }
        .into());
    }
    let mut doc = read_bookmark(req.root, req.session)?;
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
    doc["transcript"] = json!(transcript.display().to_string());
    doc["messages"] = json!(until);
    if doc.get("until").is_none() {
        doc["until"] = json!(0);
    }
    if !due {
        out.pending = doc["pending"]["prompt"].as_str().map(str::to_string);
    }
    let mut writes = Writes::new();
    if due {
        let date = crate::state::now()[..10].to_string();
        let source = format!(
            "sources/{}/{date}_{}_{from}-{until}.md",
            req.slug, req.session
        );
        let mut body = format!("# Session {} ({from}-{until})\n\nIdentity: {}. Transcript: {}. Messages {from} to {until}.{}\n", req.session, req.slug, transcript.display(), if done == 0 { String::new() } else { format!(" Messages before {done} were synced already.") });
        for (i, m) in messages[from..until].iter().enumerate() {
            let id =
                m.id.as_ref()
                    .map(|id| format!(" ({id})"))
                    .unwrap_or_default();
            body.push_str(&format!("\n## {} {}{id}\n\n{}\n", from + i, m.role, m.text));
        }
        let slice = req.root.join(&source);
        writes = writes.file(&slice, &slice, body);
        let prompt = format!(".owo/sessions/{}.prompt.md", req.session);
        let project = req
            .project
            .map(|p| format!(" --project {p}"))
            .unwrap_or_default();
        let root = fs::canonicalize(req.root).unwrap_or_else(|_| req.root.to_path_buf());
        let quoted = format!("'{}'", root.display().to_string().replace('\'', "'\\''"));
        let text = format!(
            "Use owo ingest (the ingest path of the owo skill) for identity `{slug}`{proj}.\n\nSource: `{source}` (relative to the identities root `{root}`).{earlier}\n\n1. `{owo} context{project}` and `{owo} state`.\n2. Route each durable fact in the source to its home; add `{cite}#L<a>-L<b>` to each entry's frontmatter `sources`.\n3. Run the checkpoint: `{owo} stamp && {owo} index && {owo} lint --changed`.\n4. Report changed paths or \"no changes\".\n5. When the checkpoint passes, finish the run with `owo --root {quoted} sync done --session {session} --until {until}`.\n",
            slug = req.slug,
            earlier = if done == 0 { String::new() } else { format!(" Messages before {done} were ingested in an earlier run; the first part is overlap for context.") },
            root = root.display(),
            owo = format!("owo --root {quoted} -i {}", req.slug),
            proj = req.project.map(|p| format!(" on project `{p}`")).unwrap_or_default(),
            cite = source.trim_start_matches("sources/"),
            session = req.session,
        );
        let file = req.root.join(&prompt);
        writes = writes.file(&file, &file, text);
        doc["pending"] =
            json!({ "from": from, "until": until, "source": source, "prompt": prompt });
        out.source = Some(source);
        out.prompt = Some(prompt);
    }
    Ok((
        out,
        writes.and(bookmark_writes(req.root, req.session, &doc)?),
    ))
}

/// A session with a pending run: its prompt and the messages the run covers.
#[derive(Debug, Serialize)]
pub struct Due {
    pub session: String,
    pub prompt: String,
    pub from: usize,
    pub until: usize,
}

/// The sessions in `<root>/.owo/sessions/` whose bookmark has a pending run, by session id.
/// With `slug`, only the runs whose slice is in `sources/<slug>/`.
pub fn due(root: &Path, slug: Option<&str>) -> Result<Vec<Due>> {
    let prefix = slug.map(|s| format!("sources/{s}/"));
    let dir = root.join(".owo/sessions");
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut out = vec![];
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(doc) = read_object(&path)? else {
            continue;
        };
        let bookmark = Bookmark::deserialize(&doc)
            .with_context(|| format!("{} does not parse; fix it by hand", path.display()))?;
        let of_identity =
            |p: &PendingRun| prefix.as_ref().is_none_or(|pre| p.source.starts_with(pre));
        if let Some(p) = bookmark.pending.filter(of_identity) {
            out.push(Due {
                session: bookmark.session,
                prompt: p.prompt,
                from: p.from,
                until: p.until,
            });
        }
    }
    out.sort_by(|a, b| a.session.cmp(&b.session));
    Ok(out)
}

pub fn done(root: &Path, session: &str, until: usize) -> Result<Bookmark> {
    check_session(session)?;
    let _lock = Lock::take(root, session)?;
    let mut doc = read_bookmark(root, session)?;
    if doc.get("session").is_none() {
        return Err(Fault::NoBookmark(session.into()).into());
    }
    let prev = doc["until"].as_u64().unwrap_or(0) as usize;
    if until < prev {
        return Err(Fault::BehindBookmark {
            until,
            bookmark: prev,
        }
        .into());
    }
    if let Some(seen) = doc["messages"].as_u64().filter(|n| until as u64 > *n) {
        return Err(Fault::PastMessages { until, seen }.into());
    }
    doc["until"] = json!(until);
    if doc
        .get("pending")
        .is_some_and(|p| p["until"].as_u64().is_some_and(|u| u as usize <= until))
    {
        doc.as_object_mut().expect("object").remove("pending");
    }
    let bookmark = Bookmark::deserialize(&doc)
        .with_context(|| format!("the bookmark of session {session:?} does not parse"))?;
    bookmark_writes(root, session, &doc)?.commit()?;
    Ok(bookmark)
}
