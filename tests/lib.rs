//! Tests that use only the library: the typed API with no binary, command line or rendered
//! output, and the ACP proxy on in-memory streams.

use owo::{acp, AddRequest, ContextRequest, Env, Error, Kind, Root, SearchFilter, Select};
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

/// The root `dir` with `identity` selected.
fn select(dir: &Path, identity: Option<&str>) -> Select {
    Select {
        root: Some(dir.to_path_buf()),
        identity: identity.map(str::to_string),
        project: None,
    }
}

/// The environment of a process in `dir` with no owo variables.
fn env(dir: &Path) -> Env {
    Env {
        cwd: dir.to_path_buf(),
        ..Env::default()
    }
}

/// Opens `dir` as the root, with `identity` selected.
fn open(dir: &Path, identity: Option<&str>) -> Root {
    Root::open(&select(dir, identity), &env(dir)).unwrap()
}
#[test]
fn a_root_lists_and_selects_identities() {
    let tmp = root(&["nicole", "reviewer"]);
    let r = open(tmp.path(), None);
    let slugs: Vec<String> = r.list().into_iter().map(|row| row.slug).collect();
    assert_eq!(slugs, ["nicole", "reviewer"]);
    // Two identities and none picked: loading one is a config error.
    assert!(r.config.identity.is_none());
    assert!(matches!(r.identity(), Err(Error::Config(_))));

    let r = open(tmp.path(), Some("nobody"));
    assert!(matches!(r.identity(), Err(Error::Config(_))));

    // With one identity, it is selected.
    let one = root(&["nicole"]);
    assert_eq!(
        open(one.path(), None).config.identity.as_deref(),
        Some("nicole")
    );
}

#[test]
fn read_side_calls_return_typed_results() {
    let tmp = root(&["nicole"]);
    let id = open(tmp.path(), Some("nicole")).identity().unwrap();

    let hit = id.resolve("First Firing", None).unwrap();
    assert_eq!(hit.path, "life/events/first-firing.md");
    assert!(id.resolve("no such entry at all", None).is_err());

    let (path, text) = id.show("First Firing", None).unwrap();
    assert_eq!(path, "life/events/first-firing.md");
    assert!(text.contains("title: \"First Firing\""));
    assert!(matches!(
        id.show("First Firing", Some("no such section")),
        Err(Error::Failed(_))
    ));

    let links = id.links("First Firing", true, false).unwrap();
    assert!(!links.inbound.is_empty() && links.outbound.is_empty());

    let hits = id.search(
        "glaze",
        &SearchFilter {
            scope: Some("life"),
            types: &[],
            project: None,
            since: None,
        },
        3,
    );
    assert!(!hits.is_empty() && hits.len() <= 3);
    assert!(hits.iter().all(|h| h.path.starts_with("life/")));

    let ctx = id
        .context(&ContextRequest {
            voice: true,
            look: false,
            project: None,
            with: &[],
            notes: 3,
        })
        .unwrap();
    let paths: Vec<&str> = ctx.items.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "soul/IDENTITY.md",
            "soul/SOUL.md",
            "soul/MEMORY.md",
            "soul/VOICE.md"
        ]
    );
    let text = id.context_text(&ctx);
    assert!(text.contains("<!-- owo: now -->\n## Now"));
    assert!(text.contains("<!-- owo: soul/VOICE.md (voice) -->"));

    let state = id.state(14, None).unwrap();
    assert_eq!(state.identity, "nicole");
    assert!(matches!(
        id.state(14, Some("no-such-project")),
        Err(Error::Config(_))
    ));

    assert!(id
        .lint(&[], false)
        .iter()
        .all(|d| d.level != owo::Level::Error));
    assert!(id.stamp(true).unwrap().is_clean());
    assert!(id.stale_index().is_empty());
    assert_eq!(id.review().identity, "nicole");
    assert_eq!(id.changes(None).unwrap().source, "state");
    assert!(id
        .export_card(&owo::CardRequest {
            spec: "v3",
            project: None,
            scenario: None,
            out: None,
            png: None,
        })
        .unwrap()
        .card["data"]["name"]
        .is_string());
}

#[test]
fn write_side_calls_change_the_identity() {
    let tmp = root(&["nicole"]);
    let r = open(tmp.path(), Some("nicole"));
    let id = r.identity().unwrap();

    let req = AddRequest {
        kind: Kind::Thread,
        title: "Find the glaze recipe",
        date: None,
        parent: None,
        project: None,
        aliases: &[],
        kb: None,
    };
    let (path, created) = id.add(&req).unwrap();
    assert_eq!(path, "life/threads/find-the-glaze-recipe.md");
    assert!(created);
    assert!(!id.add(&req).unwrap().1, "a second add finds the file");
    assert!(matches!(
        id.add(&AddRequest { title: " ", ..req }),
        Err(Error::Usage(_))
    ));

    assert!(id
        .write("life/journal/2026-11-05_note.md", "# A note\n")
        .unwrap());

    // A fresh load sees the new files; stamp and index pick them up.
    let id = r.identity().unwrap();
    let stamped = id.stamp(false).unwrap();
    assert!(stamped.added.contains(&path));
    id.index().unwrap();
    assert!(r.identity().unwrap().stale_index().is_empty());

    let sync = r.sync().unwrap();
    let run = sync
        .push(
            "s1",
            r#"[{"id": "1", "role": "user", "text": "hi"}]"#,
            false,
            6,
        )
        .unwrap();
    assert_eq!(run.added, Some(1));
    assert!(!run.due);
    assert!(sync.push("s1", "not json", false, 6).is_err());
}

#[test]
fn free_functions_need_no_root() {
    assert_eq!(owo::slug("Hello World").unwrap(), "hello-world");
    assert!(matches!(owo::slug("!!"), Err(Error::Failed(_))));
    assert!(owo::entry_types().any(|t| t == "event"));
    assert!(owo::lint_rules().any(|r| r == "links"));
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
    let warnings = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let proxy = acp::Proxy::new(
        &select(root, Some("nicole")),
        &env(root),
        options,
        Shared(warnings.clone()),
    )
    .unwrap();
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
    let id = open(tmp.path(), Some("nicole")).identity().unwrap();
    let context = id.context_text(
        &id.context(&ContextRequest {
            voice: false,
            look: false,
            project: None,
            with: &[],
            notes: 3,
        })
        .unwrap(),
    );
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
