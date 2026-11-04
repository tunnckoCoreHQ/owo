//! End-to-end tests for sync, migrate, kb links and the export skill.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn owo(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("owo").unwrap();
    cmd.arg("--root")
        .arg(root)
        .env_remove("OWO_ROOT")
        .env_remove("OWO_IDENTITY")
        .env_remove("OWO_PROJECT");
    cmd
}

fn json(cmd: &mut Command) -> Value {
    let out = cmd.arg("--json").output().unwrap();
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)))
}

fn copy_example(id: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let from = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(id);
    for entry in walkdir::WalkDir::new(&from) {
        let entry = entry.unwrap();
        let target = tmp
            .path()
            .join(id)
            .join(entry.path().strip_prefix(&from).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
    tmp
}

#[test]
fn sync_start_and_done() {
    let tmp = copy_example("thea");
    let t = tmp.path().join("t.jsonl");
    let lines: Vec<String> = (1..=4)
        .flat_map(|i| {
            [
                format!(r#"{{"role":"user","text":"hi {i}"}}"#),
                format!(r#"{{"role":"assistant","text":"hello {i}"}}"#),
            ]
        })
        .collect();
    fs::write(&t, lines.join("\n")).unwrap();
    let run = |args: &[&str]| json(owo(tmp.path()).args(["-i", "thea", "sync"]).args(args));
    let tp = t.to_str().unwrap();
    let v = run(&["start", "--session", "s1", "--transcript", tp]);
    assert_eq!(v["data"]["due"], false, "4 user turns < 6");
    let v = run(&[
        "start",
        "--session",
        "s1",
        "--transcript",
        tp,
        "--every",
        "3",
    ]);
    assert_eq!(v["data"]["due"], true);
    assert_eq!(v["data"]["until"], 8);
    let src = v["data"]["source"].as_str().unwrap();
    assert!(src.starts_with("sources/thea/") && src.ends_with("_s1_0-8.md"));
    assert!(fs::read_to_string(tmp.path().join(src))
        .unwrap()
        .contains("hello 4"));
    assert!(tmp.path().join(".owo/sessions/s1.prompt.md").is_file());
    run(&["done", "--session", "s1", "--until", "8"]);
    fs::write(
        &t,
        format!(
            "{}\n{}",
            lines.join("\n"),
            r#"{"role":"user","text":"later"}"#
        ),
    )
    .unwrap();
    let v = run(&["start", "--session", "s1", "--transcript", tp, "--now"]);
    assert_eq!(v["data"]["from"], 4, "two overlap user turns");
    assert_eq!(v["data"]["until"], 9);
    owo(tmp.path())
        .args([
            "-i",
            "thea",
            "sync",
            "done",
            "--session",
            "s1",
            "--until",
            "2",
        ])
        .assert()
        .code(1);
    owo(tmp.path())
        .args([
            "-i",
            "thea",
            "sync",
            "start",
            "--session",
            "../x",
            "--transcript",
            tp,
        ])
        .assert()
        .code(1);
}

#[test]
fn sync_reads_claude_jsonl() {
    let tmp = copy_example("thea");
    let t = tmp.path().join("c.jsonl");
    fs::write(
        &t,
        [
            r#"{"type":"user","message":{"role":"user","content":"draw a cat"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"A cat it is."},{"type":"tool_use","name":"x"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"ok"}]}}"#,
            r#"{"type":"summary","summary":"x"}"#,
        ]
        .join("\n"),
    )
    .unwrap();
    let v = json(
        owo(tmp.path())
            .args([
                "-i",
                "thea",
                "sync",
                "start",
                "--session",
                "c1",
                "--format",
                "claude-jsonl",
                "--now",
                "--transcript",
            ])
            .arg(&t),
    );
    assert_eq!(v["data"]["until"], 2);
    let src = fs::read_to_string(tmp.path().join(v["data"]["source"].as_str().unwrap())).unwrap();
    assert!(src.contains("A cat it is.") && !src.contains("tool_result"));
}

#[test]
fn migrate_v1_fixture() {
    let tmp = TempDir::new().unwrap();
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1-lumen");
    let out = tmp.path().join("lumen");
    let v = json(
        Command::cargo_bin("owo")
            .unwrap()
            .args(["migrate", "v1"])
            .arg(&src)
            .arg("--out")
            .arg(&out),
    );
    assert_eq!(v["ok"], true, "{v}");
    for f in [
        "soul/VOICE.md",
        "life/characters/ada.md",
        "life/characters/gull.md",
        "life/events/the-storm.md",
        "life/log/2026-01-01.md",
        "life/log/2026-01-02.md",
        "life/journal/2026-01-01_tired.md",
        "life/lessons/trim-the-wick.md",
        "life/conflicts/ferry-day.md",
    ] {
        assert!(out.join(f).is_file(), "{f}");
    }
    assert!(tmp
        .path()
        .join("sources/lumen/2026-01-01_session.md")
        .is_file());
    let skipped = v["data"]["skipped"].to_string();
    assert!(skipped.contains("changelog.md") && skipped.contains("notes.txt"));
    let id: Value =
        serde_json::from_str(&fs::read_to_string(out.join("identity.json")).unwrap()).unwrap();
    assert_eq!(id["name"], "Lumen");
    assert_eq!(id["accounts"][0]["kind"], "blog");
    let lint = json(owo(tmp.path()).args(["-i", "lumen", "lint"]));
    assert_eq!(lint["errors"].as_array().unwrap().len(), 0, "{lint}");
    owo(tmp.path())
        .args(["-i", "lumen", "resolve", "The Ferryman"])
        .assert()
        .success()
        .stdout("life/characters/ada.md\n");
    Command::cargo_bin("owo")
        .unwrap()
        .args(["migrate", "v1"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .assert()
        .code(1);
}
