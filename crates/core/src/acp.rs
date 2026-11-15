use crate::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

const NONE: &str = "_none";

struct Setup {
    identities: Vec<(String, String)>,
    context: bool,
    server: Option<Value>,
    owo: Arc<Owo>,
}

impl Setup {
    fn identity_block(&self, id: &str) -> Option<String> {
        if !self.context {
            return Some(format!("Your identity is the owo identity \"{id}\". Before you answer, load it with the `context` tool of the owo MCP server, with the argument `identity` set to \"{id}\", and keep to it."));
        }
        self.owo
            .root(id)
            .and_then(|root| {
                let identity = root.identity()?;
                let ctx = identity.context(&crate::ContextRequest::default())?;
                Ok(identity.context_text(&ctx))
            })
            .map_err(|e| self.owo.warn(&e.to_string()))
            .ok()
    }

    fn option(&self, current: Option<&str>) -> Value {
        let mut options = vec![json!({ "value": NONE, "name": "None" })];
        options.extend(
            self.identities
                .iter()
                .map(|(slug, name)| json!({ "value": slug, "name": name })),
        );
        json!({
            "id": "identity",
            "name": "Identity",
            "description": "The owo identity this session records into",
            "category": "_owo",
            "type": "select",
            "currentValue": current.unwrap_or(NONE),
            "options": options,
        })
    }

    fn pick(&self, value: &str) -> Option<Option<String>> {
        if value == NONE {
            return Some(None);
        }
        self.identities
            .iter()
            .any(|(slug, _)| slug == value)
            .then(|| Some(value.to_string()))
    }
}

enum Pending {
    New,
    Load(String),
    Resume(String),
    Prompt(String),
    Config(String),
}

#[derive(Default)]
struct Turn {
    request: String,
    replay: bool,
    user: String,
    agent: Vec<(Option<String>, String)>,
    has_identity_block: bool,
    identity: Option<String>,
    update_after_chunk: bool,
}

impl Turn {
    fn agent_turn_to_join(&mut self, id: &Option<String>) -> Option<&mut String> {
        let message = match id {
            Some(_) => self.agent.iter_mut().find(|(i, _)| i == id),
            None if self.update_after_chunk => None,
            None => self.agent.last_mut().filter(|(i, _)| i.is_none()),
        };
        message.map(|(_, text)| text)
    }
}

struct Session {
    identity: Option<String>,
    options: Vec<Value>,
    picker: bool,
    commands: Vec<Value>,
}

struct Push {
    identity: String,
    session: String,
    turns: Vec<crate::Turn>,
}

#[derive(Default)]
struct State {
    start: Option<String>,
    pending: HashMap<String, Pending>,
    needs_identity_block: HashSet<String>,
    turns: HashMap<String, VecDeque<Turn>>,
    sessions: HashMap<String, Session>,
}

impl State {
    fn session(&mut self, id: &str) -> &mut Session {
        let start = &self.start;
        self.sessions.entry(id.into()).or_insert_with(|| Session {
            identity: start.clone(),
            options: vec![],
            commands: vec![],
            picker: true,
        })
    }

    fn options(&mut self, setup: &Setup, session: &str) -> Value {
        let s = self.session(session);
        let mut all = s.options.clone();
        all.push(setup.option(s.identity.as_deref()));
        all.into()
    }

    fn commands(&mut self, session: &str) -> Value {
        let mut all = self.session(session).commands.clone();
        all.push(json!({ "name": "owo", "description": "Switch the owo identity of this session", "input": { "hint": "identity" } }));
        all.into()
    }

    fn switch(&mut self, session: &str, to: Option<String>) -> bool {
        let s = self.session(session);
        if s.identity == to {
            return false;
        }
        s.identity = to;
        if s.identity.is_some() {
            self.needs_identity_block.insert(session.into());
        } else {
            self.needs_identity_block.remove(session);
        }
        true
    }
}

fn push_of(identity: Option<String>, session: &str, turns: Vec<crate::Turn>) -> Option<Push> {
    let id = identity?;
    let name = format!("{session}.{id}");
    (!turns.is_empty()).then_some(Push {
        identity: id,
        session: name,
        turns,
    })
}

#[derive(Debug, Clone)]
pub struct Options {
    pub inject: bool,
    pub mcp_server: fn(Option<&str>) -> (String, Vec<String>),
    pub dry_run: bool,
    pub agent: Vec<String>,
}

pub fn harness_command(agent: &[String]) -> Option<Vec<String>> {
    let command: &[&str] = agent.iter().find_map(|arg| {
        // A path, or a package such as `@zed-industries/claude-agent-acp@0.5.0`.
        let name = arg.rsplit('/').next()?.split('@').next()?;
        Some(match name {
            "claude-agent-acp" | "claude-code-acp" => &[
                "claude",
                "--permission-mode",
                "acceptEdits",
                "--allowedTools",
                "Bash(owo:*)",
                "-p",
            ][..],
            "codex-acp" => &[
                "codex",
                "exec",
                "--sandbox",
                "workspace-write",
                "--skip-git-repo-check",
            ],
            "opencode" => &["opencode", "run"],
            _ => return None,
        })
    })?;
    Some(command.iter().map(|s| s.to_string()).collect())
}

struct Ingest {
    command: Vec<String>,
    root: std::path::PathBuf,
    busy: Arc<AtomicBool>,
}

impl Ingest {
    fn of(config: &crate::Resolved, agent: &[String]) -> Result<Option<Ingest>> {
        if !config.auto_ingest {
            return Ok(None);
        }
        let command = config
            .ingest_command
            .clone()
            .filter(|c| !c.is_empty())
            .or_else(|| harness_command(agent))
            .ok_or_else(|| crate::Fault::NoIngestCommand(agent.join(" ")))?;
        Ok(Some(Ingest {
            command,
            root: config.root.clone(),
            busy: Arc::default(),
        }))
    }

    /// Starts the agent for the run whose prompt file is `prompt`, unless an agent runs. Its
    /// output goes nowhere, so the client never sees it. A failure is one warning line.
    fn start(&self, prompt: &str, owo: &Arc<Owo>) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let started = std::fs::read_to_string(self.root.join(prompt)).and_then(|text| {
            std::process::Command::new(&self.command[0])
                .args(&self.command[1..])
                .arg(text)
                .current_dir(&self.root)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
        });
        let mut child = match started {
            Ok(child) => child,
            Err(e) => {
                owo.warn(&format!("the ingest of {prompt} did not start: {e}"));
                self.busy.store(false, Ordering::SeqCst);
                return;
            }
        };
        let (busy, owo, prompt) = (self.busy.clone(), owo.clone(), prompt.to_string());
        std::thread::spawn(move || {
            match child.wait() {
                Ok(status) if status.success() => {}
                Ok(status) => owo.warn(&format!("the ingest of {prompt} failed: {status}")),
                Err(e) => owo.warn(&format!("the ingest of {prompt} failed: {e}")),
            }
            busy.store(false, Ordering::SeqCst);
        });
    }
}

struct Owo {
    env: crate::Env,
    select: crate::Select,
    warnings: Mutex<Box<dyn Write + Send>>,
    dry_run: bool,
    ingest: Option<Ingest>,
}

impl Owo {
    fn root(&self, identity: &str) -> crate::Result<crate::Root> {
        let select = crate::Select {
            identity: Some(identity.into()),
            ..self.select.clone()
        };
        crate::Root::open(&select, &self.env)
    }

    fn write(&self, text: &str) {
        let mut w = self.warnings.lock().unwrap();
        let _ = w.write_all(text.as_bytes()).and_then(|_| w.flush());
    }

    fn warn(&self, text: &str) {
        self.write(&crate::warning_line(text));
    }
}

pub struct Proxy {
    owo: Arc<Owo>,
    setup: Option<Arc<Setup>>,
    start: Option<String>,
}

impl Proxy {
    pub fn new(
        select: &crate::Select,
        env: &crate::Env,
        options: Options,
        warnings: impl Write + Send + 'static,
    ) -> Result<Self> {
        let root = crate::Root::open(select, env)?;
        let owo = Arc::new(Owo {
            env: env.clone(),
            select: crate::Select {
                identity: None,
                ..select.clone()
            },
            warnings: Mutex::new(Box::new(warnings)),
            dry_run: options.dry_run,
            ingest: Ingest::of(&root.config, &options.agent)?.filter(|_| !options.dry_run),
        });
        root.warnings.iter().for_each(|w| owo.warn(w));
        let start = root.config.identity.clone();
        if start.is_some() {
            root.identity()?.working_project()?;
        }
        let identities: Vec<_> = root.list().into_iter().map(|r| (r.slug, r.name)).collect();
        let context = options.inject;
        if identities.is_empty() && context {
            return Err(crate::Fault::InjectNeedsIdentity.into());
        }
        let setup = (!identities.is_empty()).then(|| {
            let server = (!context).then(|| {
                let (command, args) = (options.mcp_server)(start.as_deref());
                json!({ "name": "owo", "command": command, "args": args, "env": [] })
            });
            Arc::new(Setup {
                identities,
                context,
                server,
                owo: owo.clone(),
            })
        });
        Ok(Self { owo, setup, start })
    }

    pub fn run(
        self,
        client_in: impl BufRead + Send + 'static,
        client_out: impl Write + Send + 'static,
        mut agent_in: impl Write + Send + 'static,
        mut agent_out: impl BufRead,
    ) -> std::io::Result<()> {
        let Proxy { owo, setup, start } = self;
        let client_out = Arc::new(Mutex::new(client_out));
        let state = Arc::new(Mutex::new(State {
            start,
            ..State::default()
        }));

        let (client_setup, client_state, client_emit) =
            (setup.clone(), state.clone(), client_out.clone());
        std::thread::spawn(move || {
            let mut client_in = client_in;
            let mut line = vec![];
            while client_in.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
                let mut msg: Option<Value> = serde_json::from_slice(&line).ok();
                let reply = client_setup
                    .as_deref()
                    .zip(msg.as_mut())
                    .and_then(|(setup, msg)| from_client(msg, &client_state, setup));
                let bytes = match (reply, &msg) {
                    (Some(Reply::Answer(lines)), _) => {
                        for l in &lines {
                            let _ = emit(&client_emit, &encode(l));
                        }
                        line.clear();
                        continue;
                    }
                    (Some(Reply::Changed), Some(msg)) => encode(msg),
                    _ => std::mem::take(&mut line),
                };
                if agent_in
                    .write_all(&bytes)
                    .and_then(|_| agent_in.flush())
                    .is_err()
                {
                    break;
                }
                line.clear();
            }
        });

        let (pusher, pushes) = mpsc::channel::<Push>();
        let pushing = std::thread::spawn(move || run_pushes(&owo, pushes));

        // Agent to client. The state changes, and a finished turn is pushed, before the
        // line is written, so the client's next request already sees them.
        let copied = (|| -> std::io::Result<()> {
            let mut line = vec![];
            while agent_out.read_until(b'\n', &mut line)? > 0 {
                track_push_then_write(&line, &state, setup.as_deref(), &pusher, &client_out)?;
                line.clear();
            }
            Ok(())
        })();
        let mut st = state.lock().unwrap();
        let open: Vec<_> = st.turns.drain().collect();
        for (session, open) in open {
            for turn in open {
                if let Some(push) = push_of(turn.identity.clone(), &session, turns(turn)) {
                    let _ = pusher.send(push);
                }
            }
        }
        drop((st, pusher));
        let _ = pushing.join();
        copied
    }
}

enum Reply {
    Changed,
    Answer(Vec<Value>),
}

fn from_client(msg: &mut Value, state: &Mutex<State>, setup: &Setup) -> Option<Reply> {
    let method = msg.get("method")?.as_str()?.to_string();
    let id = msg.get("id")?.clone();
    let key = id.to_string();
    let mut st = state.lock().unwrap();
    let params = msg.get_mut("params").filter(|p| p.is_object());
    let session = params
        .as_ref()
        .and_then(|p| p["sessionId"].as_str())
        .map(String::from);
    match method.as_str() {
        "session/new" | "session/load" | "session/resume" => {
            match (method.as_str(), &session) {
                ("session/new", _) => {
                    st.pending.insert(key, Pending::New);
                }
                ("session/load", Some(s)) => {
                    let identity = st.session(s).identity.clone();
                    st.turns.entry(s.clone()).or_default().push_back(Turn {
                        request: key.clone(),
                        replay: true,
                        identity,
                        ..Default::default()
                    });
                    st.pending.insert(key, Pending::Load(s.clone()));
                }
                ("session/resume", Some(s)) => {
                    st.pending.insert(key, Pending::Resume(s.clone()));
                }
                _ => {}
            }
            let (params, server) = (params?, setup.server.as_ref()?);
            match params.get_mut("mcpServers").and_then(Value::as_array_mut) {
                // A server the client already named `owo` stays the only one.
                Some(servers) if servers.iter().any(|s| s["name"] == "owo") => return None,
                Some(servers) => servers.push(server.clone()),
                None => params["mcpServers"] = json!([server]),
            }
        }
        "session/set_config_option" => {
            let (session, params) = (session?, params?);
            if params["configId"] != "identity" {
                st.pending.insert(key, Pending::Config(session));
                return None;
            }
            let Some(to) = params["value"].as_str().and_then(|v| setup.pick(v)) else {
                let message = format!("unknown owo identity {}", params["value"]);
                return Some(Reply::Answer(vec![
                    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32602, "message": message } }),
                ]));
            };
            st.switch(&session, to);
            let options = st.options(setup, &session);
            return Some(Reply::Answer(vec![
                json!({ "jsonrpc": "2.0", "id": id, "result": { "configOptions": options } }),
            ]));
        }
        "session/prompt" => {
            let session = session?;
            let prompt = params?.get_mut("prompt")?.as_array_mut()?;
            if let Some(arg) = command(prompt) {
                return Some(Reply::Answer(run_command(
                    &mut st, setup, &session, &id, arg,
                )));
            }
            let user: Vec<&str> = typed(prompt).collect();
            let user = user.join("\n");
            let identity = st.session(&session).identity.clone();
            let text = st
                .needs_identity_block
                .remove(&session)
                .then(|| identity.as_deref().and_then(|i| setup.identity_block(i)))
                .flatten();
            let has_identity_block = text.is_some();
            st.turns
                .entry(session.clone())
                .or_default()
                .push_back(Turn {
                    request: key.clone(),
                    user,
                    has_identity_block,
                    identity,
                    ..Default::default()
                });
            st.pending.insert(key, Pending::Prompt(session.clone()));
            let text = text?;
            prompt.insert(0, json!({ "type": "text", "text": text }));
        }
        _ => return None,
    }
    Some(Reply::Changed)
}

/// The text a person typed, from the text blocks of a prompt. A client such as T3 Code adds its
/// own instructions as XML-like elements: the content of a `<user_request>` element is the
/// typed text, and a block made only of other such elements is left out.
fn typed(prompt: &[Value]) -> impl Iterator<Item = &str> {
    prompt
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .filter_map(|text| match text.split_once("<user_request>") {
            Some((_, rest)) => rest
                .split_once("</user_request>")
                .map(|(inner, _)| inner.trim()),
            None => (!only_elements(text)).then_some(text),
        })
}

/// True when the text is one or more `<some_name>...</some_name>` elements and nothing else.
/// The name has an underscore, so a pasted HTML or XML snippet is not taken for one.
fn only_elements(text: &str) -> bool {
    let mut rest = text.trim();
    let mut found = false;
    while !rest.is_empty() {
        let Some((name, _)) = rest.strip_prefix('<').and_then(|r| r.split_once('>')) else {
            return false;
        };
        if !name.contains('_') || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return false;
        }
        let Some((_, after)) = rest.split_once(&format!("</{name}>")) else {
            return false;
        };
        (rest, found) = (after.trim_start(), true);
    }
    found
}

fn command(prompt: &[Value]) -> Option<&str> {
    let text = typed(prompt).next()?;
    let (name, arg) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    (name == "/owo").then_some(arg.trim())
}

fn run_command(st: &mut State, setup: &Setup, session: &str, id: &Value, arg: &str) -> Vec<Value> {
    let to = match arg {
        "" => None,
        "none" => Some(None),
        _ => setup.pick(arg),
    };
    let changed = to.clone().is_some_and(|to| st.switch(session, to));
    let current = match &st.session(session).identity {
        Some(i) => format!("\"{i}\""),
        None => "no identity".into(),
    };
    let text = if to.is_some() {
        format!("This session now records into {current}.")
    } else {
        let names: Vec<&str> = setup
            .identities
            .iter()
            .map(|(slug, _)| slug.as_str())
            .chain(["none"])
            .collect();
        let unknown = match arg {
            "" => String::new(),
            _ => format!("There is no owo identity \"{arg}\". "),
        };
        format!(
            "{unknown}This session records into {current}. Switch with /owo <name>: {}.",
            names.join(", ")
        )
    };
    let mut out = vec![update(
        session,
        json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } }),
    )];
    if changed && st.session(session).picker {
        let options = st.options(setup, session);
        out.push(update(
            session,
            json!({ "sessionUpdate": "config_option_update", "configOptions": options }),
        ));
    }
    out.push(json!({ "jsonrpc": "2.0", "id": id, "result": { "stopReason": "end_turn" } }));
    out
}

fn update(session: &str, update: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": session, "update": update } })
}

#[derive(Default)]
struct Agent {
    line: Option<Value>,
    after: Option<Value>,
    push: Option<Push>,
}

fn track_push_then_write(
    line: &[u8],
    state: &Mutex<State>,
    setup: Option<&Setup>,
    pusher: &mpsc::Sender<Push>,
    client_out: &Mutex<impl Write>,
) -> std::io::Result<()> {
    let Agent {
        line: changed,
        after,
        push,
    } = setup
        .and_then(|s| from_agent_line(line, state, s))
        .unwrap_or_default();
    if let Some(push) = push {
        let _ = pusher.send(push);
    }
    match &changed {
        Some(msg) => emit(client_out, &encode(msg))?,
        None => emit(client_out, line)?,
    }
    if let Some(msg) = &after {
        emit(client_out, &encode(msg))?;
    }
    Ok(())
}

fn from_agent_line(line: &[u8], state: &Mutex<State>, setup: &Setup) -> Option<Agent> {
    let mut msg: Value = serde_json::from_slice(line).ok()?;
    let mut st = state.lock().unwrap();
    if msg["method"] == "session/update" {
        let session = msg["params"]["sessionId"].as_str()?.to_string();
        let update = &mut msg["params"]["update"];
        track(&mut st, &session, update);
        match update["sessionUpdate"].as_str() {
            Some("config_option_update") => {
                st.session(&session).options = update["configOptions"].as_array()?.clone();
                update["configOptions"] = st.options(setup, &session);
            }
            Some("available_commands_update") => {
                st.session(&session).commands = update["availableCommands"].as_array()?.clone();
                update["availableCommands"] = st.commands(&session);
            }
            _ => return None,
        }
        return Some(Agent {
            line: Some(msg),
            ..Agent::default()
        });
    }
    if msg.get("method").is_some() {
        return None;
    }
    let request = msg.get("id")?.to_string();
    let error = msg.get("error").is_some();
    let mut out = Agent::default();
    let (session, new) = match st.pending.remove(&request)? {
        Pending::New => (msg["result"]["sessionId"].as_str()?.to_string(), true),
        Pending::Resume(session) => (session, false),
        Pending::Load(session) => {
            out.push = finish(&mut st, &session, &request, error);
            (session, false)
        }
        Pending::Prompt(session) => {
            out.push = finish(&mut st, &session, &request, error);
            return Some(out);
        }
        Pending::Config(session) => {
            let agent = msg["result"]["configOptions"].as_array()?.clone();
            st.session(&session).options = agent;
            msg["result"]["configOptions"] = st.options(setup, &session);
            out.line = Some(msg);
            return Some(out);
        }
    };
    if !msg["result"].is_object() {
        return Some(out);
    }
    if new && st.session(&session).identity.is_some() {
        st.needs_identity_block.insert(session.clone());
    }
    let agent = msg["result"]["configOptions"].as_array().cloned();
    // A client that gets `configOptions` hides the older `modes` field, so an agent with
    // only `modes` gets no picker. The `owo` command still switches the identity.
    let picker = agent.is_some() || msg["result"].get("modes").is_none();
    st.session(&session).picker = picker;
    st.session(&session).options = agent.unwrap_or_default();
    if picker {
        msg["result"]["configOptions"] = st.options(setup, &session);
    }
    let commands = st.commands(&session);
    out.after = Some(update(
        &session,
        json!({ "sessionUpdate": "available_commands_update", "availableCommands": commands }),
    ));
    out.line = Some(msg);
    Some(out)
}

fn track(st: &mut State, session: &str, update: &Value) -> Option<()> {
    let turn = st.turns.get_mut(session)?.front_mut()?;
    if update["sessionUpdate"] != "agent_message_chunk" || update["content"]["type"] != "text" {
        turn.update_after_chunk = true;
        return None;
    }
    let text = update["content"]["text"].as_str()?;
    let id = update["messageId"].as_str().map(String::from);
    match turn.agent_turn_to_join(&id) {
        Some(joined) => joined.push_str(text),
        None => turn.agent.push((id, text.to_string())),
    }
    turn.update_after_chunk = false;
    Some(())
}

fn finish(st: &mut State, session: &str, request: &str, error: bool) -> Option<Push> {
    let open = st.turns.get_mut(session)?;
    let turn = open.remove(open.iter().position(|t| t.request == request)?)?;
    if turn.has_identity_block && error {
        st.needs_identity_block.insert(session.into());
    }
    push_of(turn.identity.clone(), session, turns(turn))
}

fn turns(turn: Turn) -> Vec<crate::Turn> {
    let replay = turn.replay;
    let user = (!replay).then(|| crate::Turn {
        id: turn
            .agent
            .iter()
            .find_map(|(id, _)| id.as_ref())
            .map(|id| format!("{id}.user"))
            .unwrap_or_else(clock_id),
        role: crate::Role::User,
        text: turn.user,
    });
    let agent = turn.agent.into_iter().filter_map(|(id, text)| {
        let id = match id {
            Some(id) => id,
            None if !replay => clock_id(),
            None => return None,
        };
        Some(crate::Turn {
            id,
            role: crate::Role::Assistant,
            text,
        })
    });
    user.into_iter().chain(agent).collect()
}

fn clock_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("acp-{nanos}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

fn encode(msg: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(msg).expect("JSON serializes");
    bytes.push(b'\n');
    bytes
}

fn emit(out: &Mutex<impl Write>, bytes: &[u8]) -> std::io::Result<()> {
    let mut out = out.lock().unwrap();
    out.write_all(bytes)?;
    out.flush()
}

fn run_pushes(owo: &Arc<Owo>, pushes: mpsc::Receiver<Push>) {
    let push_all = || pushes.iter().for_each(|p| push(owo, p));
    if !owo.dry_run {
        return push_all();
    }
    let ((), paths) = crate::write::dry_run(push_all);
    for path in paths {
        owo.write(&format!("owo: would write {path}\n"));
    }
}

fn push(
    owo: &Arc<Owo>,
    Push {
        identity,
        session,
        turns,
    }: Push,
) {
    let run = owo.root(&identity).and_then(|root| {
        root.sync()?
            .push(&session, &turns, &crate::SyncOptions::default())
    });
    match run {
        Ok(run) => {
            if let Some((ingest, prompt)) = owo.ingest.as_ref().zip(run.prompt.or(run.pending)) {
                ingest.start(&prompt, owo);
            }
        }
        Err(e) => owo.warn(&e.to_string()),
    }
}
