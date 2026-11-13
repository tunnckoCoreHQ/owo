//! One test per finding of the PR #10 bug hunts. Each test starts in a process folder of its
//! own, apart from `Env.cwd`, so a path that resolves against the process folder shows.

mod common;

use chrono::NaiveDate;
use common::{copy_tree, examples, owo, PNG_1X1};
use owo_core::{
    mcp, parse_turns, AddRequest, Env, ErrorKind, Identity, Kind, ReviewDoneRequest, Root, Select,
    SkillRequest, SoulHistoryRequest, SoulspecRequest, StateRequest, SyncStartRequest,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tempfile::TempDir;

/// The process folder of every test here: a folder of its own, never deleted.
static PROCESS: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = TempDir::new().unwrap().keep();
    std::env::set_current_dir(&dir).unwrap();
    dir
});

/// None of `names` written into the process folder.
fn process_untouched(names: &[&str]) {
    for name in names {
        assert!(
            !PROCESS.join(name).exists(),
            "the process folder holds {name}"
        );
    }
}

/// A folder holding `ids/nicole` and `work/`, the caller's folder.
fn setup() -> TempDir {
    LazyLock::force(&PROCESS);
    let tmp = TempDir::new().unwrap();
    copy_tree(&examples().join("nicole"), &tmp.path().join("ids/nicole"));
    fs::create_dir_all(tmp.path().join("work")).unwrap();
    tmp
}

fn env(cwd: &Path) -> Env {
    Env {
        cwd: cwd.to_path_buf(),
        ..Env::default()
    }
}

fn select(tmp: &Path) -> Select {
    Select {
        root: Some(tmp.join("ids")),
        identity: Some("nicole".into()),
        project: None,
    }
}

fn identity(tmp: &Path) -> Identity {
    Root::open(&select(tmp), &env(&tmp.join("work")))
        .unwrap()
        .identity()
        .unwrap()
}

/// The tool result of one MCP call: (text, isError).
fn call(name: &str, arguments: Value, sel: &Select, env: &Env) -> (String, bool) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } });
    let r = mcp::reply(msg.to_string().as_bytes(), sel, env).unwrap();
    let text = r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    (text, r["result"]["isError"] == true)
}

fn usage<T>(r: owo_core::Result<T>) {
    let Err(e) = r else { panic!("not an error") };
    assert_eq!(e.kind, ErrorKind::Usage, "{e}");
}

fn v1(dir: &Path) {
    fs::create_dir_all(dir.join("references")).unwrap();
}

fn transcript(path: &Path) {
    fs::write(path, "{\"role\":\"user\",\"text\":\"hi\"}\n").unwrap();
}

#[test]
fn pr10_empty_required_path_is_a_usage_error() {
    let tmp = setup();
    let work = tmp.path().join("work");
    let (sel, env) = (select(tmp.path()), env(&work));
    let (text, error) = call("export_skill", json!({ "out": "" }), &sel, &env);
    assert!(error, "{text}");
    let id = identity(tmp.path());
    usage(id.export_skill(&SkillRequest::default()));
    usage(id.export_soulspec(&SoulspecRequest::default()));
    let root = Root::open(&sel, &env).unwrap();
    transcript(&work.join("t.jsonl"));
    usage(root.sync().unwrap().start(&SyncStartRequest {
        session: "s1".into(),
        transcript: PathBuf::new(),
        ..Default::default()
    }));
    let out = tmp.path().join("lumen");
    let (text, error) = call("migrate_v1", json!({ "path": "", "out": out }), &sel, &env);
    assert!(error && text.contains("missing argument"), "{text}");
    assert!(!out.exists());
    assert_eq!(fs::read_dir(&work).unwrap().count(), 1, "only t.jsonl");
    process_untouched(&["SKILL.md", "SOUL.md", "IDENTITY.md", "t.jsonl"]);
}

#[test]
fn pr10_relative_paths_join_env_cwd() {
    let tmp = setup();
    let work = tmp.path().join("work");
    let (sel, env) = (select(tmp.path()), env(&work));
    fs::write(work.join("a.png"), PNG_1X1).unwrap();
    transcript(&work.join("t.jsonl"));
    v1(&work.join("v1"));
    for (name, args) in [
        ("export_skill", json!({ "out": "sk" })),
        ("export_soulspec", json!({ "out": "pkg" })),
        ("export_card", json!({ "out": "card.json" })),
        ("export_card", json!({ "out": "card.png", "png": "a.png" })),
        (
            "sync_start",
            json!({ "session": "s1", "transcript": "t.jsonl" }),
        ),
        ("migrate_v1", json!({ "path": "v1", "out": "lumen" })),
        ("migrate_v1", json!({ "path": "v1", "out": "r/lumen" })),
    ] {
        let (text, error) = call(name, args.clone(), &sel, &env);
        assert!(!error, "{name} {args}: {text}");
    }
    for f in [
        "sk/SKILL.md",
        "pkg/SOUL.md",
        "card.json",
        "card.png",
        "lumen/identity.json",
        "r/lumen/identity.json",
    ] {
        assert!(work.join(f).is_file(), "{f}");
    }
    process_untouched(&["sk", "pkg", "card.json", "card.png", "lumen", "r"]);
}

#[test]
fn pr10_last_soul_change_is_never_an_older_record() {
    let tmp = setup();
    let path = tmp.path().join("ids/nicole/soul/state.json");
    let mut doc: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    doc["history"] = json!([
        { "at": "2026-01-01T00:00:00Z", "file": "SOUL.md", "change": "old", "evidence": [] },
        { "at": "2026-02-01T00:00:00Z", "file": "SOUL.md", "change": "new", "evidence": "[[x]]" },
    ]);
    fs::write(&path, doc.to_string()).unwrap();
    let id = identity(tmp.path());
    let history = id.soul_history(&SoulHistoryRequest::default()).unwrap();
    let last = serde_json::to_value(history.last().unwrap()).unwrap();
    assert_eq!(last["change"], "new", "{last}");
    assert_eq!(last["evidence"], "[[x]]", "{last}");
    let state = id.state(&StateRequest::default()).unwrap();
    let last = serde_json::to_value(&state.last_soul_change).unwrap();
    assert_eq!(last["change"], "new", "{last}");
    let last = serde_json::to_value(&id.review().last_soul_change).unwrap();
    assert_eq!(last["change"], "new", "{last}");
}

#[test]
fn pr10_date_outside_four_digit_years_is_a_usage_error() {
    let tmp = setup();
    let id = identity(tmp.path());
    let far = NaiveDate::from_ymd_opt(12345, 1, 1).unwrap();
    let state = tmp.path().join("ids/nicole/soul/state.json");
    let before = fs::read_to_string(&state).unwrap();
    usage(id.add(&AddRequest {
        kind: Kind::Log,
        date: Some(far),
        ..Default::default()
    }));
    assert!(!tmp
        .path()
        .join("ids/nicole/life/log/+12345-01-01.md")
        .exists());
    usage(id.review_done(&ReviewDoneRequest { through: Some(far) }));
    assert_eq!(fs::read_to_string(&state).unwrap(), before);
}

/// A root set by `.owo.toml` in the parent of the caller's folder.
fn parent_root() -> (TempDir, Select, Env) {
    let tmp = setup();
    fs::write(
        tmp.path().join(".owo.toml"),
        "root = \"ids\"\nidentity = \"nicole\"\n",
    )
    .unwrap();
    let env = env(&tmp.path().join("work"));
    (tmp, Select::default(), env)
}

#[test]
fn pr10_json_tools_keep_the_parent_root_warning() {
    let (_tmp, sel, env) = parent_root();
    for (name, args) in [
        ("list", json!({})),
        ("state", json!({})),
        ("config", json!({})),
    ] {
        let (text, _) = call(name, args, &sel, &env);
        assert!(text.contains("the root comes from"), "{name}: {text}");
    }
}

#[test]
fn pr10_failed_text_tool_keeps_the_parent_root_warning() {
    let (_tmp, sel, env) = parent_root();
    let (text, error) = call("context", json!({ "project": "nope" }), &sel, &env);
    assert!(error, "{text}");
    assert!(text.contains("the root comes from"), "{text}");
}

#[test]
fn pr10_multi_value_array_names_the_field_type() {
    let tmp = setup();
    let (sel, env) = (select(tmp.path()), env(&tmp.path().join("work")));
    let (text, error) = call("state", json!({ "days": [3, 4] }), &sel, &env);
    assert!(error && text.contains("must be an integer"), "{text}");
    let (text, error) = call(
        "context",
        json!({ "format": ["paths", "text"] }),
        &sel,
        &env,
    );
    assert!(error && !text.contains("must be a string"), "{text}");
    assert!(text.contains("paths, text"), "{text}");
}

#[cfg(unix)]
#[test]
fn pr10_cli_keeps_raw_path_bytes() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let tmp = setup();
    let work = tmp.path().join("work");
    let bad = |s: &[u8]| OsStr::from_bytes(s).to_os_string();
    v1(&work.join(bad(b"v1\xff")));
    let out = owo(&tmp.path().join("ids"))
        .current_dir(&work)
        .args(["migrate", "v1"])
        .arg(bad(b"v1\xff"))
        .args(["--out", "root/lumen"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(work.join("root/lumen/identity.json").is_file());
    transcript(&work.join(bad(b"t\xff.jsonl")));
    let out = owo(&tmp.path().join("ids"))
        .current_dir(&work)
        .args([
            "-i",
            "nicole",
            "sync",
            "start",
            "--session",
            "s1",
            "--transcript",
        ])
        .arg(bad(b"t\xff.jsonl"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The stderr of a failed `owo` run in `work`, the caller's folder.
fn cli_error(tmp: &Path, args: &[&str]) -> String {
    let out = owo(&tmp.join("ids"))
        .current_dir(tmp.join("work"))
        .args(args)
        .output()
        .unwrap();
    assert!(!out.status.success(), "{args:?} succeeded");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A read-only folder `ro` in `work`.
#[cfg(unix)]
fn read_only(tmp: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let ro = tmp.join("work/ro");
    fs::create_dir(&ro).unwrap();
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
}

#[cfg(unix)]
#[test]
fn pr10_export_write_error_names_the_path_as_given() {
    let tmp = setup();
    read_only(tmp.path());
    let err = cli_error(
        tmp.path(),
        &["-i", "nicole", "export", "card", "--out", "ro/x.json"],
    );
    assert!(err.starts_with("owo: writing ro/x.json.tmp: "), "{err}");
}

#[test]
fn pr10_export_folder_error_names_the_path_as_given() {
    let tmp = setup();
    fs::write(tmp.path().join("work/afile"), "").unwrap();
    let err = cli_error(
        tmp.path(),
        &["-i", "nicole", "export", "card", "--out", "afile/x.json"],
    );
    assert!(err.starts_with("owo: creating afile: "), "{err}");
}

#[cfg(unix)]
#[test]
fn pr10_migrate_create_error_names_the_path_as_given() {
    let tmp = setup();
    read_only(tmp.path());
    v1(&tmp.path().join("work/v1"));
    let err = cli_error(tmp.path(), &["migrate", "v1", "v1", "--out", "ro/lumen"]);
    assert!(err.starts_with("owo: creating ro/lumen: "), "{err}");
}

#[test]
fn pr10_migrate_existing_error_names_the_path_as_given() {
    let tmp = setup();
    v1(&tmp.path().join("work/v1"));
    fs::create_dir_all(tmp.path().join("work/r/nicole")).unwrap();
    let err = cli_error(tmp.path(), &["migrate", "v1", "v1", "--out", "r/nicole"]);
    assert_eq!(err.trim_end(), "owo: already exists: r/nicole");
}

#[test]
fn pr10_push_with_a_relative_env_cwd_reads_the_stored_turns() {
    LazyLock::force(&PROCESS);
    let tmp = TempDir::new_in(&*PROCESS).unwrap();
    copy_tree(&examples().join("nicole"), &tmp.path().join("ids/nicole"));
    fs::create_dir_all(tmp.path().join("work")).unwrap();
    let name = tmp.path().file_name().unwrap();
    let env = env(&Path::new(name).join("work"));
    let sel = Select {
        root: Some("../ids".into()),
        identity: Some("nicole".into()),
        project: None,
    };
    let turns = parse_turns(r#"[{"id":"a","role":"user","text":"hi"}]"#).unwrap();
    let run = Root::open(&sel, &env)
        .unwrap()
        .sync()
        .unwrap()
        .push("s1", &turns, &Default::default())
        .unwrap();
    assert_eq!(run.added, Some(1));
}

#[cfg(unix)]
#[test]
fn pr10_soulspec_write_error_names_the_path_as_given() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = setup();
    let pkg = tmp.path().join("work/pkg");
    fs::create_dir_all(pkg.join("examples")).unwrap();
    fs::set_permissions(&pkg, fs::Permissions::from_mode(0o555)).unwrap();
    let err = cli_error(
        tmp.path(),
        &["-i", "nicole", "export", "soulspec", "--out", "pkg"],
    );
    assert!(err.contains("owo: writing pkg/soul.json.tmp: "), "{err}");
}

#[test]
fn pr10_soul_history_text_keeps_string_evidence_of_a_mixed_array() {
    let tmp = setup();
    let path = tmp.path().join("ids/nicole/soul/state.json");
    let mut doc: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    doc["history"] = json!([
        { "at": "2026-01-01T00:00:00Z", "file": "SOUL.md", "change": "c", "evidence": ["[[a]]", 3, "[[b]]"] },
    ]);
    fs::write(&path, doc.to_string()).unwrap();
    let out = owo(&tmp.path().join("ids"))
        .args(["-i", "nicole", "soul", "history"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("    evidence: [[a]], [[b]]\n"), "{text}");
}

#[cfg(unix)]
#[test]
fn pr10_new_leaves_a_broken_symlink_at_the_identity_path() {
    let tmp = setup();
    let link = tmp.path().join("ids/lumen");
    std::os::unix::fs::symlink(tmp.path().join("missing"), &link).unwrap();
    let err = cli_error(tmp.path(), &["new", "Lumen"]);
    assert!(err.starts_with("owo: already exists: "), "{err}");
    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
}
