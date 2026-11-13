//! `owo acp`: a pass-through proxy for the Agent Client Protocol (newline-delimited JSON-RPC
//! over stdio). It runs the agent as a child process and copies lines both ways as the bytes
//! it received. With an identity it changes only the lines that carry the identity injection,
//! and it stores user and agent turns with `owo sync push`, called in process.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};

/// What the proxy adds to the session requests.
struct Inject {
    /// The text that tells the agent to load the identity.
    text: String,
    /// The owo MCP server entry, with `--mcp`.
    server: Option<Value>,
}

/// A client request that waits for the agent's response.
enum Pending {
    Initialize,
    New,
    Load(String),
    Prompt(String),
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
    /// Another update, such as a tool call, came after the last message chunk.
    split: bool,
}

#[derive(Default)]
struct State {
    /// The agent advertised `sessionCapabilities.systemPrompt`.
    system_prompt: bool,
    /// Keyed by the JSON text of the request id.
    pending: HashMap<String, Pending>,
    /// Sessions from `session/new` whose first prompt still gets the identity block.
    intro: HashSet<String>,
    /// The open turns of each session, oldest first. An update belongs to the oldest one.
    turns: HashMap<String, VecDeque<Turn>>,
}

/// Runs the agent behind the proxy until it exits, and returns its exit code.
pub fn serve(cli: &owo::Cli, agent: &[String], mcp: bool) -> Result<u8> {
    let globals = crate::global_args(cli, cli.identity.clone(), cli.project.clone());
    let inject = match identity(&globals)? {
        Some(id) => Some(Inject {
            text: format!("Your identity is the owo identity \"{id}\". Before you answer, load it with the `context` tool of the owo MCP server, and keep to it."),
            server: mcp.then(
                || json!({ "name": "owo", "command": "owo", "args": ["mcp", "-i", id], "env": [] }),
            ),
        }),
        None if mcp => {
            return Err(<owo::Cli as clap::CommandFactory>::command()
                .error(
                    clap::error::ErrorKind::MissingRequiredArgument,
                    "--mcp needs an identity: pass -i, set OWO_IDENTITY, or set one in the config",
                )
                .into())
        }
        None => None,
    };
    let mut child = Command::new(&agent[0])
        .args(&agent[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("starting {}", agent[0]))?;
    let mut to_agent = child.stdin.take().expect("piped stdin");
    let mut from_agent = BufReader::new(child.stdout.take().expect("piped stdout"));
    let inject = inject.map(Arc::new);
    let state = Arc::new(Mutex::new(State::default()));

    // Client to agent. This thread may stay blocked on stdin after the agent exits.
    let (client_inject, client_state) = (inject.clone(), state.clone());
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut line = vec![];
        while stdin.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
            let changed = client_inject
                .as_deref()
                .and_then(|i| from_client(&line, &client_state, i));
            let bytes = changed.as_deref().unwrap_or(&line);
            if to_agent
                .write_all(bytes)
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
    let (store, jobs) = mpsc::channel::<(String, Vec<Value>)>();
    let storer = std::thread::spawn(move || {
        for (session, turns) in jobs {
            push(&globals, &session, turns);
        }
    });

    // Agent to client. The state changes, and a finished turn goes to the store, before the
    // line is written, so the client's next request already sees them.
    let copied = (|| -> std::io::Result<()> {
        let mut stdout = std::io::stdout().lock();
        let mut line = vec![];
        while from_agent.read_until(b'\n', &mut line)? > 0 {
            if let Some(job) = inject.as_ref().and_then(|_| from_agent_line(&line, &state)) {
                let _ = store.send(job);
            }
            stdout.write_all(&line)?;
            stdout.flush()?;
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
    for (session, open) in state.lock().unwrap().turns.drain() {
        let left: Vec<Value> = open.into_iter().flat_map(turns).collect();
        if !left.is_empty() {
            let _ = store.send((session, left));
        }
    }
    drop(store);
    let _ = storer.join();
    copied?;
    Ok(status.code().unwrap_or(1) as u8)
}

/// The identity the library resolves from the flags, `OWO_IDENTITY` and the config. An
/// identity with no folder is a config error here, before the agent starts.
fn identity(globals: &[String]) -> Result<Option<String>> {
    let run = |command: &str| -> Result<owo::Output> {
        let mut argv = globals.to_vec();
        argv.extend(["--json".into(), command.into()]);
        let (_, out) = crate::run_argv(argv, String::new())?;
        match out.error {
            Some(e) => Err(e),
            None => Ok(out),
        }
    };
    let config = run("config")?;
    eprint!("{}", crate::warnings(&config));
    let id = config.envelope["data"]["identity"].as_str();
    if id.is_some() {
        // `state` loads the identity, so it checks the folder.
        run("state")?;
    }
    Ok(id.map(String::from))
}

/// Tracks a client line and returns its new bytes when the proxy changes it.
fn from_client(line: &[u8], state: &Mutex<State>, inject: &Inject) -> Option<Vec<u8>> {
    let mut msg: Value = serde_json::from_slice(line).ok()?;
    let method = msg.get("method")?.as_str()?.to_string();
    let id = msg.get("id")?.to_string();
    let mut st = state.lock().unwrap();
    let params = msg.get_mut("params").filter(|p| p.is_object());
    let session = params
        .as_ref()
        .and_then(|p| p["sessionId"].as_str())
        .map(String::from);
    match method.as_str() {
        "initialize" => {
            st.pending.insert(id, Pending::Initialize);
            return None;
        }
        "session/new" | "session/load" | "session/resume" => {
            match (method.as_str(), &session) {
                ("session/new", _) => {
                    st.pending.insert(id, Pending::New);
                }
                ("session/load", Some(s)) => {
                    st.turns.entry(s.clone()).or_default().push_back(turn(
                        &id,
                        true,
                        String::new(),
                    ));
                    st.pending.insert(id, Pending::Load(s.clone()));
                }
                _ => {}
            }
            let params = params?;
            let mut changed = false;
            if st.system_prompt {
                let block = json!({ "type": "text", "text": inject.text });
                match params.get_mut("systemPrompt").filter(|s| !s.is_null()) {
                    None => {
                        params["systemPrompt"] = json!({ "mode": "append", "content": [block] });
                        changed = true;
                    }
                    // The client's own prompt keeps its mode. One with no content list is
                    // not valid, so it passes as it is.
                    Some(own) => {
                        if let Some(content) = own.get_mut("content").and_then(Value::as_array_mut)
                        {
                            content.push(block);
                            changed = true;
                        }
                    }
                }
            }
            if let Some(server) = &inject.server {
                match params.get_mut("mcpServers").and_then(Value::as_array_mut) {
                    // A server the client already named `owo` stays the only one.
                    Some(servers) if servers.iter().any(|s| s["name"] == "owo") => {}
                    Some(servers) => {
                        servers.push(server.clone());
                        changed = true;
                    }
                    None => {
                        params["mcpServers"] = json!([server]);
                        changed = true;
                    }
                }
            }
            if !changed {
                return None;
            }
        }
        "session/prompt" => {
            let session = session?;
            let prompt = params?.get_mut("prompt")?.as_array_mut()?;
            let user: Vec<&str> = prompt
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect();
            let user = user.join("\n");
            st.turns
                .entry(session.clone())
                .or_default()
                .push_back(turn(&id, false, user));
            st.pending.insert(id, Pending::Prompt(session.clone()));
            if !st.intro.remove(&session) {
                return None;
            }
            prompt.insert(0, json!({ "type": "text", "text": inject.text }));
        }
        _ => return None,
    }
    let mut bytes = serde_json::to_vec(&msg).expect("JSON serializes");
    if line.ends_with(b"\n") {
        bytes.push(b'\n');
    }
    Some(bytes)
}

fn turn(request: &str, replay: bool, user: String) -> Turn {
    Turn {
        request: request.into(),
        replay,
        user,
        agent: vec![],
        split: false,
    }
}

/// Tracks an agent line, and returns the session and turns to store when a turn completes.
fn from_agent_line(line: &[u8], state: &Mutex<State>) -> Option<(String, Vec<Value>)> {
    let msg: Value = serde_json::from_slice(line).ok()?;
    let mut st = state.lock().unwrap();
    if msg["method"] == "session/update" {
        let (params, update) = (&msg["params"], &msg["params"]["update"]);
        let turn = st
            .turns
            .get_mut(params["sessionId"].as_str()?)?
            .front_mut()?;
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
        return None;
    }
    if msg.get("method").is_some() {
        return None;
    }
    let result = &msg["result"];
    let request = msg.get("id")?.to_string();
    match st.pending.remove(&request)? {
        Pending::Initialize => {
            st.system_prompt =
                result["agentCapabilities"]["sessionCapabilities"]["systemPrompt"].is_object();
            None
        }
        Pending::New => {
            if !st.system_prompt {
                st.intro.insert(result["sessionId"].as_str()?.to_string());
            }
            None
        }
        Pending::Load(session) | Pending::Prompt(session) => {
            let open = st.turns.get_mut(&session)?;
            let turn = open.remove(open.iter().position(|t| t.request == request)?)?;
            let turns = turns(turn);
            (!turns.is_empty()).then_some((session, turns))
        }
    }
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

/// Stores the turns with `owo sync push`. A failure is one warning line on stderr.
fn push(globals: &[String], session: &str, turns: Vec<Value>) {
    let mut argv = globals.to_vec();
    argv.extend([
        "sync".into(),
        "push".into(),
        format!("--session={session}"),
        // The proxy only stores turns: with this run size an ingest run is never due.
        format!("--every={}", usize::MAX),
        format!("--turns={}", Value::from(turns)),
    ]);
    let error = match crate::run_argv(argv, String::new()) {
        Ok((_, out)) => out.error.map(|e| format!("{e:#}")),
        Err(e) => Some(e.to_string().lines().next().unwrap_or_default().to_string()),
    };
    if let Some(e) = error {
        eprintln!("owo: warning: {e}");
    }
}
