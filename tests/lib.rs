//! Tests that use only the library: a consumer runs the MCP server and the ACP proxy on its own
//! streams, with no `owo` binary.

use clap::Parser;
use owo::{acp, mcp, Cli, Env};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::mpsc;
use tempfile::TempDir;

/// A temporary root holding a copy of each example identity in `ids`.
fn root(ids: &[&str]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    for id in ids {
        let from = examples.join(id);
        for entry in walkdir::WalkDir::new(&from) {
            let entry = entry.unwrap();
            let target = tmp
                .path()
                .join(id)
                .join(entry.path().strip_prefix(&from).unwrap());
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target).unwrap();
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    tmp
}

/// The environment of a process in `root` with no owo variables.
fn env(root: &Path) -> Env {
    Env {
        cwd: root.to_path_buf(),
        ..Env::default()
    }
}

fn cli(args: &[&str]) -> Cli {
    Cli::parse_from(std::iter::once("owo").chain(args.iter().copied()))
}

fn reply(message: Value, cli: &Cli, env: &Env) -> Value {
    mcp::reply(message.to_string().as_bytes(), cli, env).unwrap()
}

fn call(name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

#[test]
fn mcp_reply_answers_one_message() {
    let tmp = root(&["nicole"]);
    let (cli, env) = (
        cli(&["--root", tmp.path().to_str().unwrap(), "mcp"]),
        env(tmp.path()),
    );

    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } });
    let r = reply(init, &cli, &env);
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(r["result"]["serverInfo"]["name"], "owo");

    let r = reply(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        &cli,
        &env,
    );
    let names: Vec<&str> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"slug") && names.contains(&"review_done"));
    assert!(!names.contains(&"mcp") && !names.contains(&"acp"));

    let r = reply(call("slug", json!({ "text": "Hello World" })), &cli, &env);
    assert_eq!(r["result"]["isError"], false);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    let envelope: Value = serde_json::from_str(text).unwrap();
    assert_eq!(envelope["data"]["slug"], "hello-world");

    let r = reply(call("list", json!({})), &cli, &env);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    let envelope: Value = serde_json::from_str(text).unwrap();
    assert_eq!(envelope["data"]["identities"][0]["slug"], "nicole");

    let r = reply(call("context", json!({ "identity": "nicole" })), &cli, &env);
    assert_eq!(r["result"]["isError"], false);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Nicole"));

    let r = reply(call("context", json!({ "identity": "nobody" })), &cli, &env);
    assert_eq!(r["result"]["isError"], true);

    assert!(mcp::reply(
        b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}",
        &cli,
        &env
    )
    .is_none());
    assert_eq!(
        mcp::reply(b"not json", &cli, &env).unwrap()["error"]["code"],
        -32700
    );
}

#[test]
fn mcp_serves_a_reader_and_a_writer() {
    let tmp = root(&["nicole"]);
    let (cli, env) = (
        cli(&["--root", tmp.path().to_str().unwrap(), "mcp"]),
        env(tmp.path()),
    );
    let input = format!(
        "{}\n\n{}\n{}\n",
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        call("slug", json!({ "text": "a b" })),
    );
    let mut output = vec![];
    mcp::serve(input.as_bytes(), &mut output, &cli, &env).unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
    assert_eq!(lines[1]["result"]["isError"], false);
}

/// A running proxy with a scripted agent thread. The test is the client.
struct Session {
    to_proxy: std::io::PipeWriter,
    from_proxy: BufReader<std::io::PipeReader>,
    /// Each line the agent got.
    got: mpsc::Receiver<String>,
    done: std::thread::JoinHandle<std::io::Result<()>>,
    /// The warning lines the proxy wrote.
    warnings: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

/// A writer into a shared buffer, for the warning lines.
struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Starts the proxy. The agent answers its n-th line with `replies[n]`, and ends when its
/// input ends.
fn start(root: &Path, options: acp::Options, replies: Vec<Vec<String>>) -> Session {
    let cli = cli(&[
        "--root",
        root.to_str().unwrap(),
        "-i",
        "nicole",
        "acp",
        "--",
        "agent",
    ]);
    let warnings = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let proxy = acp::Proxy::new(&cli, &env(root), options, Shared(warnings.clone())).unwrap();
    let (client_in, to_proxy) = std::io::pipe().unwrap();
    let (from_proxy, client_out) = std::io::pipe().unwrap();
    let (agent_in_read, agent_in) = std::io::pipe().unwrap();
    let (agent_out, mut agent_out_write) = std::io::pipe().unwrap();
    let (got_send, got) = mpsc::channel();
    std::thread::spawn(move || {
        let mut replies = replies.into_iter();
        for line in BufReader::new(agent_in_read).lines() {
            got_send.send(line.unwrap()).unwrap();
            for reply in replies.next().unwrap_or_default() {
                writeln!(agent_out_write, "{reply}").unwrap();
            }
        }
        // Dropping the writer ends the agent's output.
    });
    let done = std::thread::spawn(move || {
        proxy.run(
            BufReader::new(client_in),
            client_out,
            agent_in,
            BufReader::new(agent_out),
        )
    });
    Session {
        to_proxy,
        from_proxy: BufReader::new(from_proxy),
        got,
        done,
        warnings,
    }
}

impl Session {
    /// Sends one client line and returns the line the agent got.
    fn send(&mut self, line: &str) -> String {
        writeln!(self.to_proxy, "{line}").unwrap();
        self.got.recv().unwrap()
    }

    /// The next line the client gets.
    fn read(&mut self) -> String {
        let mut line = String::new();
        self.from_proxy.read_line(&mut line).unwrap();
        line.trim_end_matches('\n').to_string()
    }

    /// Ends the client's input and waits for the proxy, which stores the open turns.
    fn end(self) -> Vec<u8> {
        drop(self.to_proxy);
        self.done.join().unwrap().unwrap();
        let warnings = self.warnings.lock().unwrap().clone();
        warnings
    }
}

fn new_session() -> String {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "session/new", "params": { "cwd": "/home/u", "mcpServers": [] } }).to_string()
}

fn new_reply() -> String {
    json!({ "jsonrpc": "2.0", "id": 1, "result": { "sessionId": "s1" } }).to_string()
}

fn prompt(id: u64, text: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "session/prompt", "params": { "sessionId": "s1", "prompt": [{ "type": "text", "text": text }] } }).to_string()
}

fn chunk(message: &str, text: &str) -> String {
    json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "s1", "update": { "sessionUpdate": "agent_message_chunk", "messageId": message, "content": { "type": "text", "text": text } } } }).to_string()
}

fn end(id: u64) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": { "stopReason": "end_turn" } }).to_string()
}

#[test]
fn acp_proxy_runs_on_given_streams() {
    let tmp = root(&["nicole", "reviewer"]);
    let odd = r#"{ "jsonrpc": "2.0", "id": 9, "method": "_vendor/ask", "params": {"b": [1, 2]} }"#;
    let odd_reply = r#"{"jsonrpc":"2.0",  "id":9, "result":{"x" : 1}}"#;
    let mut p = start(
        tmp.path(),
        acp::Options::default(),
        vec![
            vec![new_reply()],
            vec![odd_reply.into()],
            vec![chunk("m1", "Hel"), chunk("m1", "lo"), end(2)],
        ],
    );

    // The default mode adds the owo MCP server entry to `session/new`.
    let got: Value = serde_json::from_str(&p.send(&new_session())).unwrap();
    assert_eq!(
        got["params"]["mcpServers"],
        json!([{ "name": "owo", "command": "owo", "args": ["mcp", "-i", "nicole"], "env": [] }])
    );
    // The picker is in the `session/new` reply, and the `owo` command follows.
    let reply: Value = serde_json::from_str(&p.read()).unwrap();
    let picker = &reply["result"]["configOptions"][0];
    assert_eq!(picker["id"], "identity");
    assert_eq!(picker["currentValue"], "nicole");
    let values: Vec<&str> = picker["options"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["value"].as_str().unwrap())
        .collect();
    assert_eq!(values, ["_none", "nicole", "reviewer"]);
    let commands: Value = serde_json::from_str(&p.read()).unwrap();
    assert_eq!(
        commands["params"]["update"]["availableCommands"][0]["name"],
        "owo"
    );

    // An unchanged line passes byte for byte, both ways.
    assert_eq!(p.send(odd), odd);
    assert_eq!(p.read(), odd_reply);

    // The first prompt carries the identity block; the turn is stored.
    let got: Value = serde_json::from_str(&p.send(&prompt(2, "hello"))).unwrap();
    let blocks = got["params"]["prompt"].as_array().unwrap();
    assert!(blocks[0]["text"]
        .as_str()
        .unwrap()
        .contains("the `context` tool of the owo MCP server"));
    assert_eq!(blocks[1]["text"], "hello");
    assert_eq!(p.read(), chunk("m1", "Hel"));
    assert_eq!(p.read(), chunk("m1", "lo"));
    assert_eq!(p.read(), end(2));

    assert!(p.end().is_empty(), "no warnings");
    let stored =
        std::fs::read_to_string(tmp.path().join(".owo/sessions/s1.nicole.turns.jsonl")).unwrap();
    let stored: Vec<Value> = stored
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        stored,
        [
            json!({ "id": "m1.user", "role": "user", "text": "hello" }),
            json!({ "id": "m1", "role": "assistant", "text": "Hello" }),
        ]
    );
}

#[test]
fn acp_inject_puts_the_context_in_the_first_prompt() {
    let tmp = root(&["nicole"]);
    let context = owo::run(
        &cli(&[
            "--root",
            tmp.path().to_str().unwrap(),
            "-i",
            "nicole",
            "context",
        ]),
        &env(tmp.path()),
        &mut std::io::empty(),
    )
    .text;
    let mut p = start(
        tmp.path(),
        acp::Options { inject: true },
        vec![vec![new_reply()], vec![end(2)], vec![end(3)]],
    );
    let got: Value = serde_json::from_str(&p.send(&new_session())).unwrap();
    assert_eq!(got["params"]["mcpServers"], json!([]));
    p.read();
    p.read();
    let got: Value = serde_json::from_str(&p.send(&prompt(2, "hello"))).unwrap();
    assert_eq!(got["params"]["prompt"][0]["text"], context);
    assert_eq!(got["params"]["prompt"][1]["text"], "hello");
    assert_eq!(p.read(), end(2));
    // Only the first prompt carries it.
    assert_eq!(p.send(&prompt(3, "again")), prompt(3, "again"));
    assert_eq!(p.read(), end(3));
    p.end();
}
