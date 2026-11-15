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
    fn text(&self, id: &str) -> Option<String> {
        if !self.context {
            return Some(format!("Your identity is the owo identity \"{id}\". Before you answer, load it with the `context` tool of the owo MCP server, with the argument `identity` set to \"{id}\", and keep to it."));
        }
        self.owo
            .context(id)
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

struct Turn {
    request: String,
    replay: bool,
    user: String,
    agent: Vec<(Option<String>, String)>,
    intro: bool,
    identity: Option<String>,
    split: bool,
}

struct Session {
    identity: Option<String>,
    options: Vec<Value>,
    picker: bool,
    commands: Vec<Value>,
}

type Job = (String, String, Vec<crate::Turn>);

#[derive(Default)]
struct State {
    start: Option<String>,
    pending: HashMap<String, Pending>,
    intro: HashSet<String>,
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
            self.intro.insert(session.into());
        } else {
            self.intro.remove(session);
        }
        true
    }
}

fn job(identity: Option<String>, session: &str, turns: Vec<crate::Turn>) -> Option<Job> {
    let id = identity?;
    let name = format!("{session}.{id}");
    (!turns.is_empty()).then_some((id, name, turns))
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

    fn context(&self, identity: &str) -> crate::Result<String> {
        let id = self.root(identity)?.identity()?;
        let ctx = id.context(&crate::ContextRequest::default())?;
        Ok(id.context_text(&ctx))
    }

    fn push(
        &self,
        identity: &str,
        session: &str,
        turns: Vec<crate::Turn>,
    ) -> crate::Result<crate::SyncRun> {
        self.root(identity)?
            .sync()?
            .push(session, &turns, &crate::SyncOptions::default())
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
        let (start, rows) = identity(&owo, &root)?;
        let identities: Vec<_> = rows.into_iter().map(|r| (r.slug, r.name)).collect();
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

        let (store, jobs) = mpsc::channel::<Job>();
        let storer = std::thread::spawn(move || {
            let store_all = || {
                for (identity, session, turns) in jobs {
                    push(&owo, &identity, &session, turns);
                }
            };
            if !owo.dry_run {
                return store_all();
            }
            let ((), paths) = crate::write::dry_run(store_all);
            for path in paths {
                owo.write(&format!("owo: would write {path}\n"));
            }
        });

        let copied = (|| -> std::io::Result<()> {
            let mut line = vec![];
            while agent_out.read_until(b'\n', &mut line)? > 0 {
                let act = setup
                    .as_deref()
                    .and_then(|s| from_agent_line(&line, &state, s))
                    .unwrap_or_default();
                if let Some(job) = act.store {
                    let _ = store.send(job);
                }
                match &act.line {
                    Some(msg) => emit(&client_out, &encode(msg))?,
                    None => emit(&client_out, &line)?,
                }
                if let Some(msg) = &act.after {
                    emit(&client_out, &encode(msg))?;
                }
                line.clear();
            }
            Ok(())
        })();
        let mut st = state.lock().unwrap();
        let open: Vec<_> = st.turns.drain().collect();
        for (session, open) in open {
            for turn in open {
                if let Some(job) = job(turn.identity.clone(), &session, turns(turn)) {
                    let _ = store.send(job);
                }
            }
        }
        drop((st, store));
        let _ = storer.join();
        copied
    }
}

fn identity(owo: &Owo, root: &crate::Root) -> Result<(Option<String>, Vec<crate::Row>)> {
    for w in &root.warnings {
        owo.warn(w);
    }
    let start = match &root.config.identity {
        Some(slug) => {
            let id = root.identity()?;
            id.working_project()?;
            Some(slug.clone())
        }
        None => None,
    };
    Ok((start, root.list()))
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
                        identity,
                        ..turn(&key, true, String::new())
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
                .intro
                .remove(&session)
                .then(|| identity.as_deref().and_then(|i| setup.text(i)))
                .flatten();
            let intro = text.is_some();
            st.turns
                .entry(session.clone())
                .or_default()
                .push_back(Turn {
                    intro,
                    identity,
                    ..turn(&key, false, user)
                });
            st.pending.insert(key, Pending::Prompt(session.clone()));
            let text = text?;
            prompt.insert(0, json!({ "type": "text", "text": text }));
        }
        _ => return None,
    }
    Some(Reply::Changed)
}

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

fn turn(request: &str, replay: bool, user: String) -> Turn {
    Turn {
        request: request.into(),
        replay,
        user,
        agent: vec![],
        split: false,
        intro: false,
        identity: None,
    }
}

#[derive(Default)]
struct Agent {
    line: Option<Value>,
    after: Option<Value>,
    store: Option<Job>,
}

fn from_agent_line(line: &[u8], state: &Mutex<State>, setup: &Setup) -> Option<Agent> {
    let mut msg: Value = serde_json::from_slice(line).ok()?;
    let mut st = state.lock().unwrap();
    if msg["method"] == "session/update" {
        let session = msg["params"]["sessionId"].as_str()?.to_string();
        let update = &mut msg["params"]["update"];
        track(&mut st, &session, update);
        let options = match update["sessionUpdate"].as_str() {
            Some("config_option_update") => true,
            Some("available_commands_update") => false,
            _ => return None,
        };
        let field = if options {
            "configOptions"
        } else {
            "availableCommands"
        };
        let agent = update[field].as_array()?.clone();
        update[field] = if options {
            st.session(&session).options = agent;
            st.options(setup, &session)
        } else {
            st.session(&session).commands = agent;
            st.commands(&session)
        };
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
            out.store = finish(&mut st, &session, &request, error);
            (session, false)
        }
        Pending::Prompt(session) => {
            out.store = finish(&mut st, &session, &request, error);
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
        st.intro.insert(session.clone());
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
        turn.split = true;
        return None;
    }
    let text = update["content"]["text"].as_str()?;
    let id = update["messageId"].as_str().map(String::from);
    let same = match &id {
        Some(_) => turn.agent.iter_mut().find(|(i, _)| *i == id),
        None if turn.split => None,
        None => turn.agent.last_mut().filter(|(i, _)| i.is_none()),
    };
    turn.split = false;
    match same {
        Some((_, joined)) => joined.push_str(text),
        None => turn.agent.push((id, text.to_string())),
    }
    Some(())
}

fn finish(st: &mut State, session: &str, request: &str, error: bool) -> Option<Job> {
    let open = st.turns.get_mut(session)?;
    let turn = open.remove(open.iter().position(|t| t.request == request)?)?;
    if turn.intro && error {
        st.intro.insert(session.into());
    }
    job(turn.identity.clone(), session, turns(turn))
}

fn turns(turn: Turn) -> Vec<crate::Turn> {
    let mut out = vec![];
    if !turn.replay {
        let id = turn
            .agent
            .iter()
            .find_map(|(id, _)| id.as_ref())
            .map(|id| format!("{id}.user"))
            .unwrap_or_else(clock_id);
        out.push(crate::Turn {
            id,
            role: crate::Role::User,
            text: turn.user,
        });
    }
    for (id, text) in turn.agent {
        let id = match id {
            Some(id) => id,
            None if !turn.replay => clock_id(),
            None => continue,
        };
        out.push(crate::Turn {
            id,
            role: crate::Role::Assistant,
            text,
        });
    }
    out
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

fn push(owo: &Arc<Owo>, identity: &str, session: &str, turns: Vec<crate::Turn>) {
    match owo.push(identity, session, turns) {
        Ok(run) => {
            if let Some((ingest, prompt)) = owo.ingest.as_ref().zip(run.prompt.or(run.pending)) {
                ingest.start(&prompt, owo);
            }
        }
        Err(e) => owo.warn(&e.to_string()),
    }
}
