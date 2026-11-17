use owo_core::{acp, Env, ErrorKind, Fault, Role, Root, Select, SyncOptions, Turn};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn root(config: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/nicole");
    for entry in walkdir::WalkDir::new(&from) {
        let entry = entry.unwrap();
        let target = tmp
            .path()
            .join("nicole")
            .join(entry.path().strip_prefix(&from).unwrap());
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
    std::fs::write(tmp.path().join(".owo.toml"), config).unwrap();
    tmp
}

fn select(dir: &Path) -> Select {
    Select {
        root: Some(dir.to_path_buf()),
        identity: Some("nicole".into()),
        project: None,
    }
}

fn env(dir: &Path) -> Env {
    Env {
        cwd: dir.to_path_buf(),
        ..Env::default()
    }
}

fn mcp_server(_: Option<&str>) -> (String, Vec<String>) {
    ("owo".into(), vec!["mcp".into()])
}

fn options(agent: &[&str]) -> acp::Options {
    acp::Options {
        inject: false,
        mcp_server,
        dry_run: false,
        agent: agent.iter().map(|s| s.to_string()).collect(),
    }
}

#[derive(Clone, Default)]
struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Client {
    to_proxy: std::io::PipeWriter,
    from_proxy: BufReader<std::io::PipeReader>,
    done: std::thread::JoinHandle<std::io::Result<()>>,
    warnings: Shared,
}

fn start(root: &Path) -> Client {
    start_with(root, options(&["some-agent"]))
}

fn start_with(root: &Path, options: acp::Options) -> Client {
    let warnings = Shared::default();
    let proxy = acp::Proxy::new(&select(root), &env(root), options, warnings.clone()).unwrap();
    let (client_in, to_proxy) = std::io::pipe().unwrap();
    let (from_proxy, client_out) = std::io::pipe().unwrap();
    let (agent_in_read, agent_in) = std::io::pipe().unwrap();
    let (agent_out, mut agent_out_write) = std::io::pipe().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(agent_in_read).lines() {
            let msg: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
            let id = &msg["id"];
            let reply = match msg["method"].as_str().unwrap() {
                "session/new" => {
                    vec![json!({ "jsonrpc": "2.0", "id": id, "result": { "sessionId": "s1" } })]
                }
                _ => vec![
                    json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "s1", "update": { "sessionUpdate": "agent_message_chunk", "messageId": format!("m{id}"), "content": { "type": "text", "text": "ok" } } } }),
                    json!({ "jsonrpc": "2.0", "id": id, "result": { "stopReason": "end_turn" } }),
                ],
            };
            for r in reply {
                writeln!(agent_out_write, "{r}").unwrap();
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
    let mut c = Client {
        to_proxy,
        from_proxy: BufReader::new(from_proxy),
        done,
        warnings,
    };
    c.send(&json!({ "jsonrpc": "2.0", "id": 1, "method": "session/new", "params": { "cwd": "/home/u", "mcpServers": [] } }));
    assert!(c.read().contains("\"sessionId\":\"s1\""));
    assert!(c.read().contains("available_commands_update"));
    c
}

impl Client {
    fn send(&mut self, msg: &serde_json::Value) {
        writeln!(self.to_proxy, "{msg}").unwrap();
    }

    fn read(&mut self) -> String {
        let mut line = String::new();
        self.from_proxy.read_line(&mut line).unwrap();
        line
    }

    fn prompt(&mut self, id: u64) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": "session/prompt", "params": { "sessionId": "s1", "prompt": [{ "type": "text", "text": format!("turn {id}") }] } }));
        assert!(self.read().contains("agent_message_chunk"));
        assert_eq!(
            self.read().trim(),
            json!({ "jsonrpc": "2.0", "id": id, "result": { "stopReason": "end_turn" } })
                .to_string()
        );
    }

    fn end(self) -> String {
        drop(self.to_proxy);
        let mut rest = String::new();
        let mut from = self.from_proxy;
        std::io::Read::read_to_string(&mut from, &mut rest).unwrap();
        assert_eq!(rest, "", "the client sees no line of the ingest");
        self.done.join().unwrap().unwrap();
        let w = self.warnings.0.lock().unwrap().clone();
        String::from_utf8(w).unwrap()
    }
}

fn wait_lines(path: &Path, lines: usize) -> String {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if text.lines().count() >= lines || Instant::now() > until {
            return text;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_ingest_prompt_names_owo_ingest_and_ends_with_sync_done() {
    let tmp = root("");
    let root = Root::open(&select(tmp.path()), &env(tmp.path())).unwrap();
    let turns = [Turn {
        id: "1".into(),
        role: Role::User,
        text: "hello".into(),
    }];
    let run = root
        .sync()
        .unwrap()
        .push(
            "g",
            &turns,
            &SyncOptions {
                every: Some(1),
                ..SyncOptions::default()
            },
        )
        .unwrap();
    let text = run.prompt_text.unwrap();
    assert!(
        text.starts_with("Use owo ingest"),
        "the prompt points at the ingest part of the owo skill: {text}"
    );
    assert!(!text.contains("identity-ingest"), "{text}");
    assert!(
        text.contains("sync done --session g --until 1`"),
        "the agent finishes the run itself: {text}"
    );
    assert!(!text.contains("the host runs"), "{text}");
}

#[test]
fn auto_ingest_runs_the_prompt_in_the_background() {
    let tmp = root(
        "sync_every = 1\nauto_ingest = true\ningest_command = [\"sh\", \"-c\", \"printf '%s\\\\n' \\\"$0\\\" > got.txt; pwd -P >> got.txt\"]\n",
    );
    let mut c = start(tmp.path());
    c.prompt(2);
    let warnings = c.end();
    assert!(
        warnings.lines().all(|l| l.contains("is still pending")),
        "{warnings}"
    );
    let got = wait_lines(&tmp.path().join("got.txt"), 2);
    let prompt =
        std::fs::read_to_string(tmp.path().join(".owo/sessions/s1.nicole.prompt.md")).unwrap();
    assert!(
        got.starts_with(&prompt),
        "the run's prompt is the last argument: {got}"
    );
    let cwd = std::fs::canonicalize(tmp.path()).unwrap();
    assert_eq!(
        got.lines().last().map(Path::new),
        Some(cwd.as_path()),
        "the ingest runs in the root"
    );
}

#[test]
fn auto_ingest_is_off_by_default() {
    let tmp = root("sync_every = 1\ningest_command = [\"sh\", \"-c\", \"echo x > got.txt\"]\n");
    let mut c = start(tmp.path());
    c.prompt(2);
    c.end();
    std::thread::sleep(Duration::from_millis(500));
    assert!(!tmp.path().join("got.txt").exists());
}

#[test]
fn one_ingest_runs_at_a_time_and_a_skipped_run_starts_later() {
    let tmp = root(
        "sync_every = 1\nauto_ingest = true\ningest_command = [\"sh\", \"-c\", \"echo start >> runs.txt; sleep 1; echo end >> runs.txt\"]\n",
    );
    let runs = tmp.path().join("runs.txt");
    let mut c = start(tmp.path());
    c.prompt(2);
    wait_lines(&runs, 1);
    c.prompt(3);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(std::fs::read_to_string(&runs).unwrap(), "start\n");
    wait_lines(&runs, 2);
    std::thread::sleep(Duration::from_millis(300));
    c.prompt(4);
    let warnings = c.end();
    assert!(
        warnings.lines().all(|l| l.contains("is still pending")),
        "{warnings}"
    );
    let text = wait_lines(&runs, 3);
    assert_eq!(text.matches("start").count(), 2, "{text}");
}

#[test]
fn a_pending_run_starts_on_a_later_push_that_is_not_due() {
    let tmp = root(
        "sync_every = 2\nauto_ingest = true\ningest_command = [\"sh\", \"-c\", \"echo run >> runs.txt\"]\n",
    );
    let runs = tmp.path().join("runs.txt");
    let mut c = start(tmp.path());
    c.prompt(2);
    c.prompt(3);
    wait_lines(&runs, 1);
    std::thread::sleep(Duration::from_millis(300));
    c.prompt(4);
    c.end();
    assert_eq!(wait_lines(&runs, 2), "run\nrun\n");
}

#[test]
fn a_dry_run_starts_no_ingest_of_a_pending_run() {
    let tmp = root("sync_every = 1\n");
    let mut c = start(tmp.path());
    c.prompt(2);
    c.end();
    std::fs::write(
        tmp.path().join(".owo.toml"),
        "sync_every = 2\nauto_ingest = true\ningest_command = [\"sh\", \"-c\", \"echo x > got.txt\"]\n",
    )
    .unwrap();
    let mut c = start_with(
        tmp.path(),
        acp::Options {
            dry_run: true,
            ..options(&["some-agent"])
        },
    );
    c.prompt(3);
    let warnings = c.end();
    std::thread::sleep(Duration::from_millis(500));
    assert!(!tmp.path().join("got.txt").exists(), "{warnings}");
}

#[test]
fn a_failed_ingest_is_one_warning_line() {
    let tmp = root(
        "sync_every = 1\nauto_ingest = true\ningest_command = [\"sh\", \"-c\", \"echo x > got.txt; exit 3\"]\n",
    );
    let mut c = start(tmp.path());
    c.prompt(2);
    wait_lines(&tmp.path().join("got.txt"), 1);
    std::thread::sleep(Duration::from_millis(300));
    let warnings = c.end();
    assert_eq!(warnings.lines().count(), 1, "{warnings}");
    assert!(warnings.starts_with("owo: warning: "), "{warnings}");
}

#[test]
fn an_ingest_that_leaves_its_run_pending_is_a_warning() {
    let tmp = root(
        "sync_every = 1\nauto_ingest = true\ningest_command = [\"sh\", \"-c\", \"echo x > got.txt\"]\n",
    );
    let mut c = start(tmp.path());
    c.prompt(2);
    wait_lines(&tmp.path().join("got.txt"), 1);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        c.end(),
        "owo: warning: the ingest of session s1.nicole exited and its run is still pending: .owo/sessions/s1.nicole.prompt.md\n"
    );
}

#[test]
fn an_unknown_adapter_needs_ingest_command() {
    let tmp = root("auto_ingest = true\n");
    let new = |agent: &[&str]| {
        acp::Proxy::new(
            &select(tmp.path()),
            &env(tmp.path()),
            options(agent),
            Shared::default(),
        )
    };
    let Err(e) = new(&["some-agent", "--acp"]) else {
        panic!("an unknown adapter with auto_ingest and no ingest_command starts");
    };
    assert_eq!(e.kind, ErrorKind::Config);
    assert!(matches!(e.fault, Some(Fault::NoIngestCommand(_))));
    assert!(new(&["npx", "-y", "@zed-industries/claude-agent-acp@0.5.0"]).is_ok());
}

#[test]
fn the_ingest_command_comes_from_the_adapter() {
    let of = |agent: &[&str]| {
        let agent: Vec<String> = agent.iter().map(|s| s.to_string()).collect();
        acp::ingest_command_for(&agent)
    };
    let first = |agent: &[&str]| of(agent).map(|c| c[..2].join(" "));
    assert_eq!(
        first(&["claude-agent-acp"]).as_deref(),
        Some("claude --permission-mode")
    );
    assert!(of(&["/usr/bin/claude-agent-acp"])
        .unwrap()
        .contains(&"-p".to_string()));
    assert_eq!(
        first(&["bunx", "@zed-industries/codex-acp"]).as_deref(),
        Some("codex exec")
    );
    assert_eq!(first(&["opencode", "acp"]).as_deref(), Some("opencode run"));
    assert_eq!(of(&["some-agent"]), None);
}
