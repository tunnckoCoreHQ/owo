use owo_core as req;
use owo_core::{
    acp, AddRequest, ContextFormat, ContextRequest, Env, ErrorKind, Fault, Kind, Named, Root,
    Scope, SearchRequest, Select,
};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::mpsc;
use tempfile::TempDir;

fn root(ids: &[&str]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
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

fn select(dir: &Path, identity: Option<&str>) -> Select {
    Select {
        root: Some(dir.to_path_buf()),
        identity: identity.map(str::to_string),
        project: None,
    }
}

fn env(dir: &Path) -> Env {
    Env {
        cwd: dir.to_path_buf(),
        ..Env::default()
    }
}

fn open(dir: &Path, identity: Option<&str>) -> Root {
    Root::open(&select(dir, identity), &env(dir)).unwrap()
}
#[test]
fn a_root_lists_and_selects_identities() {
    let tmp = root(&["nicole", "reviewer"]);
    let r = open(tmp.path(), None);
    let slugs: Vec<String> = r.list().into_iter().map(|row| row.slug).collect();
    assert_eq!(slugs, ["nicole", "reviewer"]);
    assert!(r.config.identity.is_none());
    assert!(matches!(r.identity(), Err(e) if e.kind == ErrorKind::Config));

    let r = open(tmp.path(), Some("nobody"));
    assert!(matches!(r.identity(), Err(e) if e.kind == ErrorKind::Config));

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

    let hit = id
        .resolve(&req::ResolveRequest {
            name: "First Firing".into(),
            kind: None,
        })
        .unwrap()
        .unwrap();
    assert_eq!(hit.path, "life/events/first-firing.md");
    assert!(id
        .resolve(&req::ResolveRequest {
            name: "no such entry at all".into(),
            kind: None
        })
        .unwrap()
        .is_err());

    let (path, text) = id
        .show(&req::ShowRequest {
            name: "First Firing".into(),
            section: None,
        })
        .unwrap();
    assert_eq!(path, "life/events/first-firing.md");
    assert!(text.contains("title: \"First Firing\""));
    assert!(matches!(
        id.show(&req::ShowRequest { name: "First Firing".into(), section: Some("no such section".into()) }),
        Err(e) if e.kind == ErrorKind::Failed
    ));

    let links = id
        .links(&req::LinksRequest {
            name: "First Firing".into(),
            inbound: true,
            ..Default::default()
        })
        .unwrap();
    assert!(!links.inbound.is_empty() && links.outbound.is_empty());

    let hits = id
        .search(&SearchRequest {
            query: "glaze".into(),
            scope: Some(Scope::Life),
            limit: 3,
            ..Default::default()
        })
        .unwrap();
    assert!(!hits.is_empty() && hits.len() <= 3);
    assert!(hits.iter().all(|h| h.path.starts_with("life/")));

    let ctx = id
        .context(&ContextRequest {
            voice: true,
            look: false,
            with: vec![],
            notes: 3,
            format: ContextFormat::Text,
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

    let state = id.state(&owo_core::StateRequest::default()).unwrap();
    assert!(matches!(
        id.state(&owo_core::StateRequest { days: owo_core::MAX_DAYS + 1 }),
        Err(e) if e.fault == Some(Fault::OutOfRange { name: "days", value: owo_core::MAX_DAYS + 1, min: 0, max: owo_core::MAX_DAYS })
    ));
    assert_eq!(state.identity, "nicole");
    let mut other = open(tmp.path(), Some("nicole")).identity().unwrap();
    other.project = Some("no-such-project".into());
    assert!(matches!(
        other.state(&owo_core::StateRequest::default()),
        Err(e) if e.fault == Some(Fault::NoProject("no-such-project".into()))
    ));
    assert!(other
        .lint(&Default::default())
        .unwrap()
        .iter()
        .all(|d| d.level != owo_core::Level::Error));

    assert!(id
        .lint(&Default::default())
        .unwrap()
        .iter()
        .all(|d| d.level != owo_core::Level::Error));
    assert!(id
        .stamp(&req::StampRequest { check: true })
        .unwrap()
        .is_clean());
    assert!(id
        .index(&req::IndexRequest { check: true })
        .unwrap()
        .is_empty());
    assert_eq!(id.review().identity, "nicole");
    assert_eq!(id.changes(&Default::default()).unwrap().source, "state");
    let both = owo_core::ChangesRequest {
        since: Some("2026-01-01".into()),
        days: Some(3),
    };
    assert!(
        matches!(id.changes(&both), Err(e) if e.fault == Some(Fault::Conflict("since", "days")))
    );
    assert!(id
        .export_card(&owo_core::CardRequest::default())
        .unwrap()
        .card["data"]["name"]
        .is_string());
    let png = owo_core::CardRequest {
        png: Some("avatar.png".into()),
        ..Default::default()
    };
    assert!(matches!(id.export_card(&png), Err(e) if e.fault == Some(Fault::Needs("png", "out"))));
}

#[test]
fn write_side_calls_change_the_identity() {
    let tmp = root(&["nicole"]);
    let r = open(tmp.path(), Some("nicole"));
    let id = r.identity().unwrap();

    let req = AddRequest {
        kind: Kind::Thread,
        title: "Find the glaze recipe".into(),
        date: None,
        parent: None,
        aliases: vec![],
        kb: None,
    };
    let (path, created) = id.add(&req).unwrap();
    assert_eq!(path, "life/threads/find-the-glaze-recipe.md");
    assert!(created);
    assert!(!id.add(&req).unwrap().1, "a second add finds the file");
    assert!(matches!(
        id.add(&AddRequest { title: " ".into(), ..req }),
        Err(e) if e.fault == Some(Fault::NeedsTitle(Kind::Thread))
    ));

    assert!(id
        .write(&req::WriteRequest {
            path: "life/journal/2026-11-05_note.md".into(),
            content: Some("# A note\n".into())
        })
        .unwrap());

    let id = r.identity().unwrap();
    let stamped = id.stamp(&Default::default()).unwrap();
    assert!(stamped.added.contains(&path));
    id.index(&Default::default()).unwrap();
    assert!(r
        .identity()
        .unwrap()
        .index(&req::IndexRequest { check: true })
        .unwrap()
        .is_empty());

    let sync = r.sync().unwrap();
    let turns = owo_core::parse_turns(r#"[{"id": 1, "role": "user", "text": "hi"}]"#).unwrap();
    assert_eq!(
        turns,
        [owo_core::Turn {
            id: "1".into(),
            role: owo_core::Role::User,
            text: "hi".into()
        }]
    );
    let run = sync.push("s1", &turns, &Default::default()).unwrap();
    assert_eq!(run.added, Some(1));
    assert!(!run.due);
    assert!(owo_core::parse_turns("not json").is_err());
    let bad = owo_core::Turn {
        id: "a\u{7}".into(),
        ..turns[0].clone()
    };
    assert!(sync.push("s1", &[bad], &Default::default()).is_err());
    assert!(matches!(
        r.sync_done(&req::SyncDoneRequest { session: "s1".into(), until: 9 }),
        Err(e) if e.fault == Some(Fault::PastMessages { until: 9, seen: 1 })
    ));
    let bookmark = r
        .sync_done(&req::SyncDoneRequest {
            session: "s1".into(),
            until: 1,
        })
        .unwrap();
    assert_eq!((bookmark.until, bookmark.messages), (1, Some(1)));
}

#[test]
fn free_functions_need_no_root() {
    assert_eq!(
        owo_core::slug(&req::SlugRequest {
            text: "Hello World".into()
        })
        .unwrap(),
        "hello-world"
    );
    assert!(
        matches!(owo_core::slug(&req::SlugRequest { text: "!!".into() }), Err(e) if e.kind == ErrorKind::Failed)
    );
    assert!(owo_core::entry_types().any(|t| t == Kind::Event));
    assert!(owo_core::Rule::all().any(|r| r.name() == "links"));
}
struct Session {
    to_proxy: std::io::PipeWriter,
    from_proxy: BufReader<std::io::PipeReader>,
    got: mpsc::Receiver<String>,
    done: std::thread::JoinHandle<std::io::Result<()>>,
    warnings: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn mcp_server(identity: Option<&str>) -> (String, Vec<String>) {
    let mut args = vec!["mcp".to_string()];
    if let Some(id) = identity {
        args.extend(["-i".to_string(), id.to_string()]);
    }
    ("owo".to_string(), args)
}

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
    fn send(&mut self, line: &str) -> String {
        writeln!(self.to_proxy, "{line}").unwrap();
        self.got.recv().unwrap()
    }

    fn read(&mut self) -> String {
        let mut line = String::new();
        self.from_proxy.read_line(&mut line).unwrap();
        line.trim_end_matches('\n').to_string()
    }

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
        acp::Options {
            inject: false,
            mcp_server,
            dry_run: false,
            agent: vec![],
        },
        vec![
            vec![new_reply()],
            vec![odd_reply.into()],
            vec![chunk("m1", "Hel"), chunk("m1", "lo"), end(2)],
        ],
    );

    let got: Value = serde_json::from_str(&p.send(&new_session())).unwrap();
    assert_eq!(
        got["params"]["mcpServers"],
        json!([{ "name": "owo", "command": "owo", "args": ["mcp", "-i", "nicole"], "env": [] }])
    );
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

    assert_eq!(p.send(odd), odd);
    assert_eq!(p.read(), odd_reply);

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
            with: vec![],
            notes: 3,
            format: ContextFormat::Text,
        })
        .unwrap(),
    );
    let mut p = start(
        tmp.path(),
        acp::Options {
            inject: true,
            mcp_server,
            dry_run: false,
            agent: vec![],
        },
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
    assert_eq!(p.send(&prompt(3, "again")), prompt(3, "again"));
    assert_eq!(p.read(), end(3));
    p.end();
}

#[test]
fn acp_push_writes_the_run_when_due() {
    let tmp = root(&["nicole"]);
    let mut replies = vec![vec![new_reply()]];
    replies.extend((2..=7).map(|i| vec![chunk(&format!("m{i}"), "ok"), end(i)]));
    let mut p = start(
        tmp.path(),
        acp::Options {
            inject: false,
            mcp_server,
            dry_run: false,
            agent: vec![],
        },
        replies,
    );
    p.send(&new_session());
    p.read();
    p.read();
    for i in 2..=7 {
        p.send(&prompt(i, &format!("turn {i}")));
        p.read();
        p.read();
    }
    assert!(p.end().is_empty(), "no warnings");
    let due = open(tmp.path(), Some("nicole")).sync_due().unwrap();
    assert_eq!(due.len(), 1, "six user turns make a run due");
    assert_eq!(due[0].session, "s1.nicole");
    assert_eq!((due[0].from, due[0].until), (0, 12));
    assert!(tmp.path().join(&due[0].prompt).is_file());
}

#[test]
fn acp_dry_run_stores_no_turns() {
    let tmp = root(&["nicole"]);
    let mut replies = vec![vec![new_reply()]];
    replies.extend((2..=7).map(|i| vec![chunk(&format!("m{i}"), "ok"), end(i)]));
    let mut p = start(
        tmp.path(),
        acp::Options {
            inject: false,
            mcp_server,
            dry_run: true,
            agent: vec![],
        },
        replies,
    );
    p.send(&new_session());
    p.read();
    p.read();
    for i in 2..=7 {
        p.send(&prompt(i, &format!("turn {i}")));
        p.read();
        p.read();
    }
    let warnings = String::from_utf8(p.end()).unwrap();
    let sessions = tmp.path().join(".owo/sessions");
    for path in [
        sessions.join("s1.nicole.turns.jsonl"),
        sessions.join("s1.nicole.json"),
        sessions.join("s1.nicole.prompt.md"),
    ] {
        let line = format!("owo: would write {}\n", path.display());
        assert!(warnings.contains(&line), "{warnings}");
    }
    let slice = format!(
        "owo: would write {}",
        tmp.path().join("sources/nicole/").display()
    );
    assert!(warnings.contains(&slice), "{warnings}");
    assert!(!tmp.path().join(".owo").exists());
    assert!(!tmp.path().join("sources").exists());
}
