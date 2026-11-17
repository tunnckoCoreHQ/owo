use crate::identity::{read_object, SESSIONS, SOURCES};
use crate::write::Writes;
use crate::{Fault, Named};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

named! {
    #[derive(Default)]
    pub enum Format {
        #[default]
        Jsonl = "jsonl",
        ClaudeJsonl = "claude-jsonl",
        OpencodeJson = "opencode-json",
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncCursor {
    pub session: String,
    pub transcript: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<usize>,
    pub until: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingRun>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PendingRun {
    pub from: usize,
    pub until: usize,
    pub slice: String,
    pub prompt: String,
}

const OVERLAP_TURNS: usize = 2;
const STALE_LOCK: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct TranscriptTurn {
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

fn id_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn parse(raw: &str, format: Format) -> Result<Vec<TranscriptTurn>> {
    let mut out = vec![];
    let mut push = |id: Option<String>, role: Option<&str>, text: String| {
        if let Some(role @ ("user" | "assistant")) = role {
            if !text.trim().is_empty() {
                out.push(TranscriptTurn {
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

#[derive(Debug, Default, Serialize)]
pub struct SyncReport {
    pub due: bool,
    pub identity: String,
    pub session: String,
    pub new_user_turns: usize,
    pub every: usize,
    pub from: usize,
    pub until: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slice_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_text: Option<String>,
}

pub struct StartRequest<'a> {
    pub identities_dir: &'a Path,
    pub dir: &'a Path,
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

struct Lock {
    path: PathBuf,
    _made: Made,
}

struct Made(Vec<PathBuf>);

impl Drop for Made {
    fn drop(&mut self) {
        for dir in &self.0 {
            let _ = fs::remove_dir(dir);
        }
    }
}

impl Lock {
    fn take(dir: &Path, session: &str) -> Result<Option<Lock>> {
        if crate::write::dry() {
            return Ok(None);
        }
        let path = dir.join(SESSIONS).join(format!("{session}.lock"));
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
                    let stale = fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .is_ok_and(|t| t.elapsed().unwrap_or_default() > STALE_LOCK);
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

fn cursor_path(dir: &Path, session: &str) -> PathBuf {
    dir.join(SESSIONS).join(format!("{session}.json"))
}

fn read_cursor(dir: &Path, session: &str) -> Result<Value> {
    Ok(read_object(&cursor_path(dir, session))?.unwrap_or_else(|| json!({})))
}

fn turns_file(session: &str) -> String {
    format!("{SESSIONS}/{session}.turns.jsonl")
}

pub fn start(req: &StartRequest, transcript: &Path, format: Format) -> Result<SyncReport> {
    check_session(req.session)?;
    let _lock = Lock::take(req.dir, req.session)?;
    if let Some(t) = read_cursor(req.dir, req.session)?["transcript"].as_str() {
        if t.ends_with(&turns_file(req.session)) {
            return Err(Fault::PushedSession(req.session.into()).into());
        }
    }
    let raw = fs::read_to_string(req.cwd.join(transcript))
        .with_context(|| format!("reading {}", transcript.display()))?;
    let (out, writes) = run(req, transcript, &raw, format)?;
    writes.commit()?;
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub id: String,
    pub role: Role,
    pub text: String,
}

named! {
    pub enum Role {
        User = "user",
        Assistant = "assistant",
    }
}

fn check_turn_id(n: usize, id: &str) -> Result<()> {
    if id.is_empty() || id.chars().any(char::is_control) {
        bail!("turn {n}: the turn id must be a number or a string with no control characters");
    }
    Ok(())
}

pub fn parse_turns(json: &str) -> Result<Vec<Turn>> {
    let list: Value = serde_json::from_str(json).context("turns are not JSON")?;
    let list = list
        .as_array()
        .context("turns must be a JSON array of {id, role, text}")?;
    let mut turns = vec![];
    for (n, t) in list.iter().enumerate() {
        let id = id_of(&t["id"]).unwrap_or_default();
        check_turn_id(n, &id)?;
        let Some(role) = t["role"].as_str().and_then(Role::from_name) else {
            bail!("turn {n}: role must be user or assistant");
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

fn drop_cut_off_line(bytes: &mut Vec<u8>) {
    bytes.truncate(bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1));
}

pub fn push(req: &StartRequest, turns: &[Turn]) -> Result<SyncReport> {
    check_session(req.session)?;
    let _lock = Lock::take(req.dir, req.session)?;
    let file = req.dir.join(turns_file(req.session));
    if let Some(t) = read_cursor(req.dir, req.session)?["transcript"].as_str() {
        if !t.ends_with(&turns_file(req.session)) {
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
    drop_cut_off_line(&mut bytes);
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
    Writes::default()
        .file(&file, &file, stored)
        .and(writes)
        .commit()?;
    out.added = Some(added);
    // A host that pushes turns cannot read the files, so each reply of a pending run has both.
    let pending = &read_cursor(req.dir, req.session)?["pending"];
    let text = |key: &str| {
        let rel = pending[key].as_str()?;
        crate::write::read_to_string(&req.dir.join(rel)).ok()
    };
    out.slice_text = text("slice");
    out.prompt_text = text("prompt");
    Ok(out)
}

fn new_user_turns(turns: &[TranscriptTurn], done: usize, pending_end: Option<usize>) -> usize {
    let counted_from = pending_end.map_or(done, |end| end.clamp(done, turns.len()));
    turns[counted_from..]
        .iter()
        .filter(|t| t.role == "user")
        .count()
}

fn run(
    req: &StartRequest,
    transcript: &Path,
    raw: &str,
    format: Format,
) -> Result<(SyncReport, Writes)> {
    let turns = parse(raw, format)?;
    if turns.is_empty() && !raw.trim().is_empty() {
        return Err(Fault::NoTurns {
            transcript: transcript.to_path_buf(),
            format: format.name(),
        }
        .into());
    }
    let mut doc = read_cursor(req.dir, req.session)?;
    let done = doc["until"].as_u64().unwrap_or(0) as usize;
    let done = done.min(turns.len());
    let until = turns.len();
    let pending_end = doc["pending"]["until"].as_u64().map(|u| u as usize);
    let new_user = new_user_turns(&turns, done, pending_end);
    let due = new_user > 0 && (req.now || new_user >= req.every);
    let from = (0..done)
        .rev()
        .filter(|&i| turns[i].role == "user")
        .nth(OVERLAP_TURNS - 1)
        .unwrap_or(0);
    let mut out = SyncReport {
        due,
        identity: req.slug.into(),
        session: req.session.into(),
        new_user_turns: new_user,
        every: req.every,
        from,
        until,
        ..SyncReport::default()
    };
    doc["session"] = json!(req.session);
    doc["transcript"] = json!(transcript.display().to_string());
    doc["turns"] = json!(until);
    if doc.get("until").is_none() {
        doc["until"] = json!(0);
    }
    if !due {
        out.pending = doc["pending"]["prompt"].as_str().map(str::to_string);
    }
    let mut writes = Writes::default();
    if due {
        let date = crate::state::now()[..10].to_string();
        let cite = format!("{date}_{}_{from}-{until}.md", req.session);
        let slice = format!("{SOURCES}/{cite}");
        let mut body = format!("# Session {} ({from}-{until})\n\nIdentity: {}. Transcript: {}. Turns {from} to {until}.{}\n", req.session, req.slug, transcript.display(), if done == 0 { String::new() } else { format!(" Turns before {done} were synced already.") });
        for (i, t) in turns[from..until].iter().enumerate() {
            let id =
                t.id.as_ref()
                    .map(|id| format!(" ({id})"))
                    .unwrap_or_default();
            body.push_str(&format!("\n## {} {}{id}\n\n{}\n", from + i, t.role, t.text));
        }
        let file = req.dir.join(&slice);
        writes = writes.file(&file, &file, body);
        let prompt = format!("{SESSIONS}/{}.prompt.md", req.session);
        let project = req
            .project
            .map(|p| format!(" --project {p}"))
            .unwrap_or_default();
        let real = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let identities_dir = real(req.identities_dir).display().to_string();
        let quoted = format!("'{}'", identities_dir.replace('\'', "'\\''"));
        let text = format!(
            "Use owo ingest (the ingest path of the owo skill) for identity `{slug}`{proj}.\n\nRaw material: the slice `{slice}` (relative to the identity folder `{dir}`).{earlier}\n\nRun every owo operation of this run on this identities dir and identity. If you read this prompt from a file, start each command, the commands of the skill too, with the exact prefix `{owo}`. If it came in a `sync_push` result, use the tools of that same MCP server with `identity` set to `{slug}`, such as `sync_done` for `owo sync done`; its `guide` tool gives the owo skill. Do not use any other owo server or connector.\n\n1. `{owo} context{project}` and `{owo} state`.\n2. Route each durable fact in the slice to its home; add `{cite}#L<a>-L<b>` to each entry's frontmatter `sources`.\n3. Run the checkpoint: `{owo} stamp && {owo} index && {owo} lint --changed`.\n4. Report changed paths or \"no changes\".\n5. When the checkpoint passes, finish the run with `{owo} sync done --session {session} --until {until}`.\n",
            slug = req.slug,
            earlier = if done == 0 { String::new() } else { format!(" Turns before {done} were ingested in an earlier run; the first part is overlap for context.") },
            dir = real(req.dir).display(),
            owo = format!("owo --identities-dir {quoted} -i {}", req.slug),
            proj = req.project.map(|p| format!(" on project `{p}`")).unwrap_or_default(),
            session = req.session,
        );
        let file = req.dir.join(&prompt);
        writes = writes.file(&file, &file, text);
        doc["pending"] = json!({ "from": from, "until": until, "slice": slice, "prompt": prompt });
        out.slice = Some(slice);
        out.prompt = Some(prompt);
    }
    let path = cursor_path(req.dir, req.session);
    Ok((out, writes.and(Writes::default().json(&path, &path, &doc)?)))
}

#[derive(Debug, Serialize)]
pub struct PendingSession {
    pub identity: String,
    pub session: String,
    pub prompt: String,
    pub from: usize,
    pub until: usize,
}

/// The pending runs of one identity, or of every identity in the identities dir.
pub fn due(identities_dir: &Path, slug: Option<&str>) -> Result<Vec<PendingSession>> {
    let slugs = match slug {
        Some(slug) => vec![slug.to_string()],
        None => crate::config::list_identities(identities_dir),
    };
    let mut out = vec![];
    for slug in slugs {
        let dir = identities_dir.join(&slug).join(SESSIONS);
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Some(doc) = read_object(&path)? else {
                continue;
            };
            let cursor = SyncCursor::deserialize(&doc)
                .with_context(|| format!("{} does not parse; fix it by hand", path.display()))?;
            if let Some(p) = cursor.pending {
                out.push(PendingSession {
                    identity: slug.clone(),
                    session: cursor.session,
                    prompt: p.prompt,
                    from: p.from,
                    until: p.until,
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.identity, &a.session).cmp(&(&b.identity, &b.session)));
    Ok(out)
}

pub fn done(dir: &Path, session: &str, until: usize) -> Result<SyncCursor> {
    check_session(session)?;
    let _lock = Lock::take(dir, session)?;
    let mut doc = read_cursor(dir, session)?;
    if doc.get("session").is_none() {
        return Err(Fault::NoSyncCursor(session.into()).into());
    }
    let prev = doc["until"].as_u64().unwrap_or(0) as usize;
    if until < prev {
        return Err(Fault::BehindSyncCursor {
            until,
            cursor: prev,
        }
        .into());
    }
    if let Some(seen) = doc["turns"].as_u64().filter(|n| until as u64 > *n) {
        return Err(Fault::PastTurns { until, seen }.into());
    }
    doc["until"] = json!(until);
    if doc
        .get("pending")
        .is_some_and(|p| p["until"].as_u64().is_some_and(|u| u as usize <= until))
    {
        doc.as_object_mut().expect("object").remove("pending");
    }
    let cursor = SyncCursor::deserialize(&doc)
        .with_context(|| format!("the sync cursor of session {session:?} does not parse"))?;
    let path = cursor_path(dir, session);
    Writes::default().json(&path, &path, &doc)?.commit()?;
    Ok(cursor)
}
