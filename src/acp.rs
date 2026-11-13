//! `owo acp`: a pass-through proxy for the Agent Client Protocol (newline-delimited JSON-RPC
//! over stdio). It runs the agent as a child process and copies lines both ways as the bytes
//! it received. When the root has identities, it gives each session an identity the client can
//! switch with the `identity` config option or the `/owo` command, changes only the lines that
//! carry the injection, the picker and the command, and stores user and agent turns with
//! `owo sync push`, called in process.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

/// The picker value for no identity.
const NONE: &str = "_none";

/// What the proxy knows from the start.
struct Setup {
    /// The slug and name of each identity in the root.
    identities: Vec<(String, String)>,
    /// True with `--inject`: the identity block is the output of `owo context`.
    context: bool,
    /// The owo MCP server entry. None with `--inject`.
    server: Option<Value>,
    /// The `--root` and `--project` flags of each owo call.
    globals: Vec<String>,
}

impl Setup {
    /// The identity block for `id`. By default it tells the agent to load the identity with the
    /// owo MCP `context` tool. With `--inject` it is the output of `owo context`; when that
    /// fails, one warning line goes to stderr and nothing is injected.
    fn text(&self, id: &str) -> Option<String> {
        if !self.context {
            return Some(format!("Your identity is the owo identity \"{id}\". Before you answer, load it with the `context` tool of the owo MCP server, with the argument `identity` set to \"{id}\", and keep to it."));
        }
        let mut argv = self.globals.clone();
        argv.extend(["-i".into(), id.into(), "context".into()]);
        run(argv)
            .map(|out| out.text)
            .map_err(|e| eprintln!("owo: warning: {e}"))
            .ok()
    }

    /// The `identity` config option with `current` selected.
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

    /// The identity a picker value names: `Some(None)` for none, None for an unknown value.
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

/// A client request that waits for the agent's response.
enum Pending {
    New,
    Load(String),
    Resume(String),
    Prompt(String),
    /// A `session/set_config_option` for an option of the agent.
    Config(String),
}

/// One open turn of a session: a `session/prompt` or a `session/load` that waits for its response.
struct Turn {
    /// The JSON text of the request id.
    request: String,
    /// True for a `session/load`, whose updates are a replay.
    replay: bool,
    user: String,
    /// Agent messages in order: the `messageId`, when there is one, and the joined text.
    agent: Vec<(Option<String>, String)>,
    /// The prompt carried the identity block.
    intro: bool,
    /// Another update, such as a tool call, came after the last message chunk.
    split: bool,
}

/// What the proxy knows of one session.
struct Session {
    /// The identity the session records into. None stores nothing and injects nothing.
    identity: Option<String>,
    /// The agent's last config option list.
    options: Vec<Value>,
    /// The agent's last command list.
    commands: Vec<Value>,
}

/// The identity, the sync session name and the turns of one store.
type Job = (String, String, Vec<Value>);

#[derive(Default)]
struct State {
    /// The identity resolved at start, which each session starts with.
    start: Option<String>,
    /// Keyed by the JSON text of the request id.
    pending: HashMap<String, Pending>,
    /// Sessions whose next prompt gets the identity block.
    intro: HashSet<String>,
    /// The open turns of each session, oldest first. An update belongs to the oldest one.
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
        })
    }

    /// The complete config option list of a session: the agent's, then the identity option.
    fn options(&mut self, setup: &Setup, session: &str) -> Value {
        let s = self.session(session);
        let mut all = s.options.clone();
        all.push(setup.option(s.identity.as_deref()));
        all.into()
    }

    /// The complete command list of a session: the agent's, then `owo`.
    fn commands(&mut self, session: &str) -> Value {
        let mut all = self.session(session).commands.clone();
        all.push(json!({ "name": "owo", "description": "Switch the owo identity of this session", "input": { "hint": "identity" } }));
        all.into()
    }

    /// Switches the identity of a session, so its next prompt carries the block for the new
    /// one. Returns false when the session already has that identity.
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

    /// The store of finished turns, under the session's current identity. None when the
    /// session has no identity or there are no turns.
    fn job(&mut self, session: &str, turns: Vec<Value>) -> Option<Job> {
        let id = self.session(session).identity.clone()?;
        let name = format!("{session}.{id}");
        (!turns.is_empty()).then_some((id, name, turns))
    }
}

/// Runs the agent behind the proxy until it exits, and returns its exit code.
pub fn serve(cli: &owo::Cli, agent: &[String], context: bool) -> Result<u8> {
    let start = identity(&crate::global_args(
        cli,
        cli.identity.clone(),
        cli.project.clone(),
    ))?;
    let identities = list(crate::global_args(cli, None, None))?;
    if identities.is_empty() && context {
        return Err(<owo::Cli as clap::CommandFactory>::command()
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                "--inject needs an identity in the root",
            )
            .into());
    }
    let globals = crate::global_args(cli, None, cli.project.clone());
    let setup = (!identities.is_empty()).then(|| {
        let args = match &start {
            Some(id) => json!(["mcp", "-i", id]),
            None => json!(["mcp"]),
        };
        Arc::new(Setup {
            identities,
            context,
            server: (!context)
                .then(|| json!({ "name": "owo", "command": "owo", "args": args, "env": [] })),
            globals: globals.clone(),
        })
    });
    let mut child = Command::new(&agent[0])
        .args(&agent[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting {}", agent[0]))?;
    let mut to_agent = child.stdin.take().expect("piped stdin");
    let mut from_agent = BufReader::new(child.stdout.take().expect("piped stdout"));
    let state = Arc::new(Mutex::new(State {
        start,
        ..State::default()
    }));

    // Client to agent. This thread may stay blocked on stdin after the agent exits. It answers
    // the requests the proxy handles itself, without waiting for the agent.
    let (client_setup, client_state) = (setup.clone(), state.clone());
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut line = vec![];
        while stdin.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
            let mut msg: Option<Value> = serde_json::from_slice(&line).ok();
            let reply = client_setup
                .as_deref()
                .zip(msg.as_mut())
                .and_then(|(setup, msg)| from_client(msg, &client_state, setup));
            let bytes = match (reply, &msg) {
                (Some(Reply::Answer(lines)), _) => {
                    for l in &lines {
                        let _ = emit(&encode(l));
                    }
                    line.clear();
                    continue;
                }
                (Some(Reply::Changed), Some(msg)) => encode(msg),
                _ => std::mem::take(&mut line),
            };
            if to_agent
                .write_all(&bytes)
                .and_then(|_| to_agent.flush())
                .is_err()
            {
                break;
            }
            line.clear();
        }
        // Dropping `to_agent` closes the agent's stdin.
    });

    // Stores run on their own thread, so they never delay the pass-through.
    let (store, jobs) = mpsc::channel::<Job>();
    let storer = std::thread::spawn(move || {
        for (identity, session, turns) in jobs {
            push(&globals, &identity, &session, turns);
        }
    });

    // Agent to client. The state changes, and a finished turn goes to the store, before the
    // line is written, so the client's next request already sees them.
    let copied = (|| -> std::io::Result<()> {
        let mut line = vec![];
        while from_agent.read_until(b'\n', &mut line)? > 0 {
            let act = setup
                .as_deref()
                .and_then(|s| from_agent_line(&line, &state, s))
                .unwrap_or_default();
            if let Some(job) = act.store {
                let _ = store.send(job);
            }
            match &act.line {
                Some(msg) => emit(&encode(msg))?,
                None => emit(&line)?,
            }
            if let Some(msg) = &act.after {
                emit(&encode(msg))?;
            }
            line.clear();
        }
        Ok(())
    })();
    // A client that closed its stdout ends the session: stop the agent, then finish the stores.
    if copied.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    // The agent is gone: store the turns it left open.
    let mut st = state.lock().unwrap();
    let open: Vec<_> = st.turns.drain().collect();
    for (session, open) in open {
        let left: Vec<Value> = open.into_iter().flat_map(turns).collect();
        if let Some(job) = st.job(&session, left) {
            let _ = store.send(job);
        }
    }
    drop((st, store));
    let _ = storer.join();
    copied?;
    Ok(status.code().unwrap_or(1) as u8)
}

/// The identity the library resolves from the flags, `OWO_IDENTITY` and the config. An
/// identity with no folder, or a project that is not found, is a config error here, before the
/// agent starts.
fn identity(globals: &[String]) -> Result<Option<String>> {
    let run = |command: &str| -> Result<owo::Output> {
        let mut argv = globals.to_vec();
        argv.extend(["--json".into(), command.into()]);
        Ok(crate::run_argv(argv, String::new())?.1)
    };
    let config = run("config")?;
    eprint!("{}", crate::warnings(&config));
    if let Some(e) = config.error {
        return Err(e);
    }
    let Some(id) = config.envelope["data"]["identity"].as_str() else {
        return Ok(None);
    };
    // `state` loads the identity, so it checks the identity folder and the project.
    match run("state")?.error {
        Some(e) => Err(e),
        None => Ok(Some(id.into())),
    }
}

/// The slug and name of each identity in the root, from `owo list --json`.
fn list(mut argv: Vec<String>) -> Result<Vec<(String, String)>> {
    argv.extend(["--json".into(), "list".into()]);
    let out = crate::run_argv(argv, String::new())?.1;
    if let Some(e) = out.error {
        return Err(e);
    }
    let all = out.envelope["data"]["identities"].as_array().cloned();
    Ok(all
        .into_iter()
        .flatten()
        .filter_map(|i| {
            let slug = i["slug"].as_str()?;
            Some((slug.into(), i["name"].as_str().unwrap_or(slug).into()))
        })
        .collect())
}

/// What the proxy does with a client line it reads.
enum Reply {
    /// The message changed: forward the new one.
    Changed,
    /// The proxy answers with these lines, and the agent never gets the request.
    Answer(Vec<Value>),
}

/// Tracks a client message, and returns what to do when the proxy does not forward it unchanged.
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
                    st.turns.entry(s.clone()).or_default().push_back(turn(
                        &key,
                        true,
                        String::new(),
                    ));
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
            let user: Vec<&str> = prompt
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect();
            let user = user.join("\n");
            let identity = st.session(&session).identity.clone();
            let text = st
                .intro
                .remove(&session)
                .then(|| identity.and_then(|i| setup.text(&i)))
                .flatten();
            let intro = text.is_some();
            st.turns
                .entry(session.clone())
                .or_default()
                .push_back(Turn {
                    intro,
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

/// The argument of a `/owo` command in the first text block of a prompt.
fn command(prompt: &[Value]) -> Option<&str> {
    let text = prompt.iter().find(|b| b["type"] == "text")?["text"].as_str()?;
    if text == "/owo" {
        return Some("");
    }
    text.strip_prefix("/owo ").map(str::trim)
}

/// Runs `/owo <arg>` and returns the lines that answer the prompt: a message, a config
/// update when the identity changed, and the end of the turn.
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
    if changed {
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
    }
}

/// What the proxy does with an agent line.
#[derive(Default)]
struct Agent {
    /// The message to write instead of the line read.
    line: Option<Value>,
    /// A message of the proxy's own to write after it.
    after: Option<Value>,
    store: Option<Job>,
}

/// Tracks an agent line. It returns the turns to store when a turn completes, and the new
/// message when the line carries a config option list or a command list.
fn from_agent_line(line: &[u8], state: &Mutex<State>, setup: &Setup) -> Option<Agent> {
    let mut msg: Value = serde_json::from_slice(line).ok()?;
    let mut st = state.lock().unwrap();
    if msg["method"] == "session/update" {
        let session = msg["params"]["sessionId"].as_str()?.to_string();
        let update = &mut msg["params"]["update"];
        track(&mut st, &session, update);
        // Both lists are complete: the proxy keeps the agent's and adds its own entry.
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
    // A session response: the picker goes after the agent's options, and the `owo` command
    // follows the response.
    if !msg["result"].is_object() {
        return Some(out);
    }
    if new && st.session(&session).identity.is_some() {
        st.intro.insert(session.clone());
    }
    let agent = msg["result"]["configOptions"].as_array().cloned();
    st.session(&session).options = agent.unwrap_or_default();
    msg["result"]["configOptions"] = st.options(setup, &session);
    let commands = st.commands(&session);
    out.after = Some(update(
        &session,
        json!({ "sessionUpdate": "available_commands_update", "availableCommands": commands }),
    ));
    out.line = Some(msg);
    Some(out)
}

/// Adds a session update to the oldest open turn of the session.
fn track(st: &mut State, session: &str, update: &Value) -> Option<()> {
    let turn = st.turns.get_mut(session)?.front_mut()?;
    if update["sessionUpdate"] != "agent_message_chunk" || update["content"]["type"] != "text" {
        turn.split = true;
        return None;
    }
    let text = update["content"]["text"].as_str()?;
    let id = update["messageId"].as_str().map(String::from);
    // Chunks with one `messageId` are one message. Chunks with none are one message until
    // another update, such as a tool call, comes between them.
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

/// Closes the open turn of a response and returns its store.
fn finish(st: &mut State, session: &str, request: &str, error: bool) -> Option<Job> {
    let open = st.turns.get_mut(session)?;
    let turn = open.remove(open.iter().position(|t| t.request == request)?)?;
    // A refused prompt did not deliver the identity block, so the next prompt carries it.
    if turn.intro && error {
        st.intro.insert(session.into());
    }
    st.job(session, turns(turn))
}

/// The `{id, role, text}` turns of a finished turn. The user turn's id is the first agent
/// `messageId` plus `.user`. A live message with no id gets a clock id; a replayed one is left out.
fn turns(turn: Turn) -> Vec<Value> {
    let mut out = vec![];
    if !turn.replay {
        let id = turn
            .agent
            .iter()
            .find_map(|(id, _)| id.as_ref())
            .map(|id| format!("{id}.user"))
            .unwrap_or_else(clock_id);
        out.push(json!({ "id": id, "role": "user", "text": turn.user }));
    }
    for (id, text) in turn.agent {
        let id = match id {
            Some(id) => id,
            None if !turn.replay => clock_id(),
            None => continue,
        };
        out.push(json!({ "id": id, "role": "assistant", "text": text }));
    }
    out
}

/// A unique id from the clock and a counter.
fn clock_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("acp-{nanos}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// One JSON-RPC message as a line.
fn encode(msg: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(msg).expect("JSON serializes");
    bytes.push(b'\n');
    bytes
}

/// Writes one whole line to the client and flushes it. Both threads write through the stdout
/// lock, so their lines never mix.
fn emit(bytes: &[u8]) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(bytes)?;
    out.flush()
}

/// Stores the turns with `owo sync push` for `identity`. A failure is one warning line on stderr.
fn push(globals: &[String], identity: &str, session: &str, turns: Vec<Value>) {
    let mut argv = globals.to_vec();
    argv.extend([
        "-i".into(),
        identity.into(),
        "sync".into(),
        "push".into(),
        format!("--session={session}"),
        // The proxy only stores turns: with this run size an ingest run is never due.
        format!("--every={}", usize::MAX),
        format!("--turns={}", Value::from(turns)),
    ]);
    if let Err(e) = run(argv) {
        eprintln!("owo: warning: {e}");
    }
}

/// Runs one owo command line in process. The error is one line of text.
fn run(argv: Vec<String>) -> Result<owo::Output, String> {
    match crate::run_argv(argv, String::new()) {
        Ok((_, out)) => match out.error {
            Some(e) => Err(format!("{e:#}")),
            None => Ok(out),
        },
        Err(e) => Err(e.to_string().lines().next().unwrap_or_default().to_string()),
    }
}
