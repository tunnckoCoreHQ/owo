use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use tempfile::TempDir;

pub struct Proxy {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: BufReader<ChildStdout>,
    pub dir: TempDir,
    pub sent: usize,
}

pub fn start(ids_dir: &Path, flags: &[&str], code: u8) -> Proxy {
    let dir = TempDir::new().unwrap();
    let agent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/acp-agent.sh");
    let mut child = Command::new(assert_cmd::cargo::cargo_bin("owo"))
        .arg("--identities-dir")
        .arg(ids_dir)
        .arg("acp")
        .args(flags)
        .arg("--")
        .arg("bash")
        .arg(agent)
        .arg(dir.path())
        .arg(code.to_string())
        .env("OWO_DIR", ids_dir.join(".owo"))
        .env_remove("OWO_CONFIG")
        .env_remove("OWO_TEMPLATES_DIR")
        .env_remove("OWO_IDENTITIES_DIR")
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
    pub fn send(&mut self, line: &str, reply: &[String]) {
        let got = self.call(line, reply, reply.len());
        for (got, want) in got.iter().zip(reply) {
            assert_eq!(got, want);
        }
    }

    pub fn call(&mut self, line: &str, reply: &[String], n: usize) -> Vec<String> {
        self.sent += 1;
        let text: String = reply.iter().map(|l| format!("{l}\n")).collect();
        fs::write(self.dir.path().join(format!("reply.{}", self.sent)), &text).unwrap();
        self.ask(line, n)
    }

    pub fn ask(&mut self, line: &str, n: usize) -> Vec<String> {
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .unwrap();
        self.stdin.flush().unwrap();
        (0..n)
            .map(|_| {
                let mut got = String::new();
                self.stdout.read_line(&mut got).unwrap();
                got.strip_suffix('\n').unwrap().to_string()
            })
            .collect()
    }

    pub fn finish(self) -> (i32, Vec<String>) {
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

pub fn s(v: Value) -> String {
    v.to_string()
}

pub fn init() -> String {
    s(
        json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": 1, "clientCapabilities": {} } }),
    )
}

pub fn init_reply(session_capabilities: Value) -> String {
    s(
        json!({ "jsonrpc": "2.0", "id": 0, "result": { "protocolVersion": 1, "agentCapabilities": { "loadSession": true, "sessionCapabilities": session_capabilities } } }),
    )
}

pub fn request(id: u64, method: &str, params: Value) -> String {
    s(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
}

pub fn response(id: u64, result: Value) -> String {
    s(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

pub fn prompt(id: u64, session: &str, texts: &[&str]) -> String {
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

pub fn update(session: &str, kind: &str, message: Option<&str>, text: &str) -> String {
    let mut u = json!({ "sessionUpdate": kind, "content": { "type": "text", "text": text } });
    if let Some(m) = message {
        u["messageId"] = json!(m);
    }
    s(
        json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": session, "update": u } }),
    )
}

pub fn chunk(session: &str, message: Option<&str>, text: &str) -> String {
    update(session, "agent_message_chunk", message, text)
}

pub fn end(id: u64) -> String {
    response(id, json!({ "stopReason": "end_turn" }))
}

pub fn parse(line: &str) -> Value {
    serde_json::from_str(line).unwrap()
}

/// The turns stored for `session`, whose name ends in its identity, such as `s.nicole`.
pub fn stored(ids_dir: &Path, session: &str) -> Vec<Value> {
    let identity = session.rsplit('.').next().unwrap();
    fs::read_to_string(ids_dir.join(format!("{identity}/sessions/{session}.turns.jsonl")))
        .unwrap()
        .lines()
        .map(parse)
        .collect()
}

pub fn turn(id: &str, role: &str, text: &str) -> Value {
    json!({ "id": id, "role": role, "text": text })
}

pub fn both() -> TempDir {
    let tmp = super::copy_example("nicole");
    super::copy_tree(
        &super::examples().join("reviewer"),
        &tmp.path().join("reviewer"),
    );
    tmp
}

pub fn picker(current: &str, ids: &[&str]) -> Value {
    let mut options = vec![json!({ "value": "_none", "name": "None" })];
    for id in ids {
        let name = if *id == "nicole" {
            "Nicole"
        } else {
            "Rook Halvorsen"
        };
        options.push(json!({ "value": id, "name": name }));
    }
    json!({ "id": "identity", "name": "Identity", "description": "The owo identity this session records into", "category": "_owo", "type": "select", "currentValue": current, "options": options })
}

pub fn commands(session: &str, agent: &[Value]) -> Value {
    let mut all = agent.to_vec();
    all.push(json!({ "name": "owo", "description": "Switch the owo identity of this session", "input": { "hint": "identity" } }));
    session_update(
        session,
        json!({ "sessionUpdate": "available_commands_update", "availableCommands": all }),
    )
}

pub fn session_update(session: &str, update: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": session, "update": update } })
}

impl Proxy {
    pub fn open(&mut self, line: &str, reply: &[String], session: &str, options: Value) {
        let got = self.call(line, reply, reply.len() + 1);
        let (last, rest) = reply.split_last().unwrap();
        assert_eq!(&got[..rest.len()], rest);
        let mut want = parse(last);
        want["result"]["configOptions"] = options;
        assert_eq!(parse(&got[rest.len()]), want);
        assert_eq!(parse(&got[reply.len()]), commands(session, &[]));
    }
}
