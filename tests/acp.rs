//! End-to-end tests for `owo acp`: a fake agent (`tests/fixtures/acp-agent.sh`) behind the proxy.

mod common;

use common::{copy_example, examples, owo};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tempfile::TempDir;

/// A running `owo acp` with the fake agent. The test is the client.
struct Proxy {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// Where the agent records the lines it gets and reads its replies.
    dir: TempDir,
    sent: usize,
}

fn start(root: &Path, flags: &[&str], code: u8) -> Proxy {
    let dir = TempDir::new().unwrap();
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/acp-agent.sh");
    let mut child = Command::new(assert_cmd::cargo::cargo_bin("owo"))
        .arg("--root")
        .arg(root)
        .arg("acp")
        .args(flags)
        .arg("--")
        .arg("bash")
        .arg(agent)
        .arg(dir.path())
        .arg(code.to_string())
        .env_remove("OWO_ROOT")
        .env_remove("OWO_IDENTITY")
        .env_remove("OWO_PROJECT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    Proxy {
        stdin: child.stdin.take().unwrap(),
        stdout: BufReader::new(child.stdout.take().unwrap()),
        child,
        dir,
        sent: 0,
    }
}

impl Proxy {
    /// Sends one client line; the agent answers it with `reply`, which must arrive byte for byte.
    fn send(&mut self, line: &str, reply: &[String]) {
        self.sent += 1;
        let text: String = reply.iter().map(|l| format!("{l}\n")).collect();
        fs::write(self.dir.path().join(format!("reply.{}", self.sent)), &text).unwrap();
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .unwrap();
        self.stdin.flush().unwrap();
        for want in reply {
            let mut got = String::new();
            self.stdout.read_line(&mut got).unwrap();
            assert_eq!(got, format!("{want}\n"));
        }
    }

    /// Closes the client's stdin and returns the exit code and the lines the agent got.
    fn finish(self) -> (i32, Vec<String>) {
        let Proxy {
            mut child,
            stdin,
            mut stdout,
            dir,
            ..
        } = self;
        drop(stdin);
        let status = child.wait().unwrap();
        let mut rest = String::new();
        stdout.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "", "no line the agent did not send");
        let got = fs::read_to_string(dir.path().join("got")).unwrap();
        (
            status.code().unwrap(),
            got.lines().map(String::from).collect(),
        )
    }
}

fn s(v: Value) -> String {
    v.to_string()
}

fn init() -> String {
    s(
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": 1, "clientCapabilities": {} } }),
    )
}

fn init_reply(session_capabilities: Value) -> String {
    s(
        json!({ "jsonrpc": "2.0", "id": 0, "result": { "protocolVersion": 1, "agentCapabilities": { "loadSession": true, "sessionCapabilities": session_capabilities } } }),
    )
}

fn request(id: u64, method: &str, params: Value) -> String {
    s(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
}

fn response(id: u64, result: Value) -> String {
    s(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

fn prompt(id: u64, session: &str, texts: &[&str]) -> String {
    let blocks: Vec<Value> = texts
        .iter()
        .map(|t| json!({ "type": "text", "text": t }))
        .collect();
    request(
        id,
        "session/prompt",
        json!({ "sessionId": session, "prompt": blocks }),
    )
}

fn update(session: &str, kind: &str, message: Option<&str>, text: &str) -> String {
    let mut u = json!({ "sessionUpdate": kind, "content": { "type": "text", "text": text } });
    if let Some(m) = message {
        u["messageId"] = json!(m);
    }
    s(
        json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": session, "update": u } }),
    )
}

fn chunk(session: &str, message: Option<&str>, text: &str) -> String {
    update(session, "agent_message_chunk", message, text)
}

fn end(id: u64) -> String {
    response(id, json!({ "stopReason": "end_turn" }))
}

fn parse(line: &str) -> Value {
    serde_json::from_str(line).unwrap()
}

fn stored(root: &Path, session: &str) -> Vec<Value> {
    fs::read_to_string(root.join(format!(".owo/sessions/{session}.turns.jsonl")))
        .unwrap()
        .lines()
        .map(parse)
        .collect()
}

fn turn(id: &str, role: &str, text: &str) -> Value {
    json!({ "id": id, "role": role, "text": text })
}

#[test]
fn passes_lines_through_and_stores_turns_once() {
    let tmp = copy_example("nicole");
    let mut p = start(tmp.path(), &[], 7);
    let first = s(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "session/prompt", "params": { "sessionId": "s1", "prompt": [
        { "type": "text", "text": "hello" },
        { "type": "resource_link", "uri": "file:///a.md", "name": "a.md" },
        { "type": "text", "text": "world" },
    ] } }),
    );
    let sent = [
        init(),
        request(
            1,
            "session/new",
            json!({ "cwd": "/home/u", "mcpServers": [] }),
        ),
        r#"{ "jsonrpc": "2.0", "id": 9, "method": "_vendor/ask", "params": {"b": [1, 2]} }"#.into(),
        r#"{"jsonrpc":"2.0","method":"_vendor/note"}"#.into(),
        "not json from the client".into(),
        first.clone(),
        prompt(3, "s1", &["again"]),
        request(
            4,
            "session/load",
            json!({ "sessionId": "s1", "cwd": "/home/u", "mcpServers": [] }),
        ),
    ];
    let replies = [
        vec![
            init_reply(json!({})),
            r#"{"jsonrpc":"2.0","method":"_vendor/hello","params":{"x":1}}"#.into(),
            "not json from the agent".into(),
        ],
        vec![response(1, json!({ "sessionId": "s1" }))],
        vec![r#"{"jsonrpc":"2.0","id":9,"error":{"code":-32601,"message":"no"}}"#.into()],
        vec![],
        vec![],
        vec![
            chunk("s1", Some("m1"), "Hel"),
            update("s1", "agent_thought_chunk", Some("t1"), "thinking"),
            chunk("s1", Some("m1"), "lo"),
            s(
                json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "s1", "update": { "sessionUpdate": "tool_call", "toolCallId": "c1", "title": "Read", "status": "pending" } } }),
            ),
            chunk("s1", Some("m2"), "Bye"),
            end(2),
        ],
        vec![chunk("s1", Some("m3"), "ok"), end(3)],
        vec![
            update("s1", "user_message_chunk", Some("u1"), "hello"),
            chunk("s1", Some("m1"), "Hello"),
            chunk("s1", Some("m2"), "Bye"),
            chunk("s1", None, "replayed with no id"),
            response(4, json!({})),
        ],
    ];
    for (line, reply) in sent.iter().zip(&replies) {
        p.send(line, reply);
    }
    let (code, got) = p.finish();
    assert_eq!(code, 7, "the agent's exit code");
    assert_eq!(got.len(), sent.len());
    for (i, (got, sent)) in got.iter().zip(&sent).enumerate() {
        if i == 5 {
            // Without the systemPrompt capability, the first prompt gets the identity block first.
            let mut got = parse(got);
            let block = got["params"]["prompt"].as_array_mut().unwrap().remove(0);
            assert_eq!(block["type"], "text");
            assert!(block["text"].as_str().unwrap().contains("\"nicole\""));
            assert_eq!(got, parse(sent));
        } else {
            assert_eq!(got, sent, "line {i} passes byte for byte");
        }
    }
    assert_eq!(
        stored(tmp.path(), "s1"),
        [
            turn("m1.user", "user", "hello\nworld"),
            turn("m1", "assistant", "Hello"),
            turn("m2", "assistant", "Bye"),
            turn("m3.user", "user", "again"),
            turn("m3", "assistant", "ok"),
        ],
        "the load replay adds nothing"
    );
}

#[test]
fn appends_the_system_prompt_and_the_mcp_server() {
    let tmp = copy_example("nicole");
    let mut p = start(tmp.path(), &["--mcp"], 0);
    let fs_server = json!({ "name": "fs", "command": "/bin/fs", "args": [], "env": [] });
    let sent = [
        init(),
        request(
            1,
            "session/new",
            json!({ "cwd": "/w", "mcpServers": [fs_server] }),
        ),
        request(
            2,
            "session/load",
            json!({ "sessionId": "s2", "cwd": "/w", "mcpServers": [], "systemPrompt": { "mode": "append", "content": [{ "type": "text", "text": "Be brief." }] } }),
        ),
        request(
            3,
            "session/resume",
            json!({ "sessionId": "s2", "cwd": "/w" }),
        ),
        prompt(4, "s2", &["hi"]),
    ];
    let replies = [
        vec![init_reply(json!({ "systemPrompt": {} }))],
        vec![response(1, json!({ "sessionId": "s2" }))],
        vec![response(2, json!({}))],
        vec![response(3, json!({}))],
        vec![chunk("s2", Some("m1"), "yo"), end(4)],
    ];
    for (line, reply) in sent.iter().zip(&replies) {
        p.send(line, reply);
    }
    let (code, got) = p.finish();
    assert_eq!(code, 0);
    assert_eq!(got[0], sent[0]);
    assert_eq!(got[4], sent[4], "a prompt passes unchanged");
    let text = parse(&got[1])["params"]["systemPrompt"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("\"nicole\"") && text.contains("`context`"));
    let block = json!({ "type": "text", "text": text });
    let owo_server =
        json!({ "name": "owo", "command": "owo", "args": ["mcp", "-i", "nicole"], "env": [] });
    let mut new = parse(&sent[1]);
    new["params"]["systemPrompt"] = json!({ "mode": "append", "content": [block] });
    new["params"]["mcpServers"] = json!([fs_server, owo_server]);
    assert_eq!(parse(&got[1]), new);
    let mut load = parse(&sent[2]);
    load["params"]["systemPrompt"]["content"] =
        json!([{ "type": "text", "text": "Be brief." }, block]);
    load["params"]["mcpServers"] = json!([owo_server]);
    assert_eq!(parse(&got[2]), load);
    let mut resume = parse(&sent[3]);
    resume["params"]["systemPrompt"] = json!({ "mode": "append", "content": [block] });
    resume["params"]["mcpServers"] = json!([owo_server]);
    assert_eq!(parse(&got[3]), resume);
    assert_eq!(
        stored(tmp.path(), "s2"),
        [turn("m1.user", "user", "hi"), turn("m1", "assistant", "yo")]
    );
}

#[test]
fn without_an_identity_it_only_passes_lines_through() {
    let tmp = TempDir::new().unwrap();
    let mut p = start(tmp.path(), &[], 0);
    let sent = [
        init(),
        request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] })),
        prompt(2, "s3", &["hi"]),
    ];
    let replies = [
        vec![init_reply(json!({ "systemPrompt": {} }))],
        vec![response(1, json!({ "sessionId": "s3" }))],
        vec![chunk("s3", Some("m1"), "yo"), end(2)],
    ];
    for (line, reply) in sent.iter().zip(&replies) {
        p.send(line, reply);
    }
    let (code, got) = p.finish();
    assert_eq!(code, 0);
    assert_eq!(got, sent);
    assert!(!tmp.path().join(".owo").exists(), "nothing is stored");
    owo(tmp.path())
        .args(["acp", "--mcp", "--", "true"])
        .assert()
        .code(2);
}

#[test]
fn mcp_does_not_list_acp() {
    let out = owo(&examples())
        .args(["-i", "nicole", "mcp"])
        .write_stdin(format!(
            "{}\n",
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })
        ))
        .output()
        .unwrap();
    let reply = parse(String::from_utf8(out.stdout).unwrap().trim());
    let tools = reply["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "sync_push"));
    assert!(!tools.iter().any(|t| t["name"] == "acp"));
}
