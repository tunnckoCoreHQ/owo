//! End-to-end tests against the committed identities in `examples/`.
//! Read-only commands run in place; commands that write run on a copy.

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn owo(root: &Path, id: &str) -> Command {
    let mut cmd = Command::cargo_bin("owo").unwrap();
    cmd.arg("--root")
        .arg(root)
        .arg("-i")
        .arg(id)
        .env_remove("OWO_ROOT")
        .env_remove("OWO_IDENTITY")
        .env_remove("OWO_PROJECT");
    cmd
}

fn json(cmd: &mut Command) -> Value {
    let out = cmd.arg("--json").output().unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn examples_are_lint_clean_and_current() {
    for id in ["thea", "reviewer"] {
        let v = json(owo(&examples(), id).arg("lint"));
        assert_eq!(v["errors"].as_array().unwrap().len(), 0, "{id}: {v}");
        assert_eq!(v["warnings"].as_array().unwrap().len(), 0, "{id}: {v}");
        owo(&examples(), id)
            .args(["index", "--check"])
            .assert()
            .success();
        owo(&examples(), id)
            .args(["stamp", "--check"])
            .assert()
            .success();
    }
}

#[test]
fn resolve_show_links() {
    let root = examples();
    owo(&root, "reviewer")
        .args(["resolve", "Mina Okafor"])
        .assert()
        .success()
        .stdout("life/characters/mina-okafor.md\n");
    owo(&root, "reviewer")
        .args(["resolve", "mina-okafr"])
        .assert()
        .code(1);
    let shown = owo(&root, "reviewer")
        .args(["show", "The Rollback", "--section", "Key facts"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&shown.stdout).contains("[[Rollback postmortem]]"));
    let v = json(owo(&root, "reviewer").args(["links", "Mina Okafor", "--in"]));
    let from: Vec<&str> = v["data"]["in"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["path"].as_str().unwrap())
        .collect();
    assert!(from.contains(&"life/events/the-rollback.md"), "{from:?}");
    let v = json(owo(&root, "thea").args(["links", "--orphans"]));
    assert_eq!(v["data"]["orphans"].as_array().unwrap().len(), 0);
}

#[test]
fn context_with_project_points_at_missing_kb() {
    let v = json(owo(&examples(), "reviewer").args([
        "--project",
        "ledger-api",
        "context",
        "--no-touch",
        "--voice",
    ]));
    let paths: Vec<&str> = v["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        &paths[..4],
        [
            "soul/IDENTITY.md",
            "soul/SOUL.md",
            "soul/MEMORY.md",
            "soul/VOICE.md"
        ]
    );
    assert!(paths.contains(&"work/README.md"));
    assert!(paths.contains(&"work/projects/ledger-api/notes/2026-10-26_rollback-postmortem.md"));
    assert!(v["data"]["kb"]["path"]
        .as_str()
        .unwrap()
        .ends_with("examples/kb/ledger-api/lat.md"));
    assert_eq!(v["data"]["kb"]["exists"], false);
    assert!(v["warnings"].to_string().contains("KB not found"));
    let now = v["data"]["now"].as_str().unwrap();
    assert!(now.contains("[[Weekly hour]] (2026-11-03)"), "{now}");
    assert!(now.contains("Open threads: [[Flaky Cache Test]]"), "{now}");
}

#[test]
fn context_project_touches_work_state_on_a_copy() {
    let tmp = TempDir::new().unwrap();
    for entry in walkdir::WalkDir::new(examples().join("thea")) {
        let entry = entry.unwrap();
        let target = tmp
            .path()
            .join("thea")
            .join(entry.path().strip_prefix(examples().join("thea")).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
    for _ in 0..2 {
        owo(tmp.path(), "thea")
            .args(["--project", "picture-book", "context", "--format", "text"])
            .assert()
            .success();
    }
    let state: Value =
        serde_json::from_str(&fs::read_to_string(tmp.path().join("thea/work/state.json")).unwrap())
            .unwrap();
    assert_eq!(state["projects"]["picture-book"]["sessions"], 2);
    let s = json(owo(tmp.path(), "thea").arg("state"));
    assert_eq!(s["data"]["last_project"][0], "picture-book");
}

#[test]
fn search_ranks_title_matches_first() {
    let v = json(owo(&examples(), "thea").args(["search", "blue", "glaze"]));
    assert_eq!(
        v["data"]["hits"][0]["path"],
        "life/threads/the-blue-glaze.md"
    );
    let v = json(owo(&examples(), "reviewer").args(["search", "idempotency", "--scope", "work"]));
    assert!(v["data"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|h| h["path"].as_str().unwrap().starts_with("work/")));
}

#[test]
fn recent_lists_stamped_files() {
    let v = json(owo(&examples(), "reviewer").args(["recent", "--days", "36500"]));
    assert!(v["data"]["files"].as_array().unwrap().len() > 20);
}
