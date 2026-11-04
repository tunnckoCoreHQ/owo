//! End-to-end tests: run the built `owo` binary against temporary roots.

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

fn copy_tree(from: &Path, to: &Path) {
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.unwrap();
        let target = to.join(entry.path().strip_prefix(from).unwrap());
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A fresh "Aria" identity with the fixture entries copied in and indexes regenerated.
fn aria() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    owo(tmp.path()).args(["new", "Aria"]).assert().success();
    let dir = tmp.path().join("aria");
    copy_tree(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aria"),
        &dir,
    );
    owo(tmp.path())
        .args(["-i", "aria", "index"])
        .assert()
        .success();
    (tmp, dir)
}

fn lint_json(root: &Path, rules: &[&str]) -> Value {
    let mut cmd = owo(root);
    cmd.args(["-i", "aria", "lint", "--json"]);
    for r in rules {
        cmd.args(["--rule", r]);
    }
    serde_json::from_slice(&cmd.output().unwrap().stdout).unwrap()
}

fn rules_in(v: &Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            format!(
                "{}:{}",
                d["rule"].as_str().unwrap(),
                d["path"].as_str().unwrap()
            )
        })
        .collect()
}

#[test]
fn new_identity_lints_clean() {
    let tmp = TempDir::new().unwrap();
    owo(tmp.path()).args(["new", "Thea"]).assert().success();
    owo(tmp.path()).args(["lint"]).assert().success();
    owo(tmp.path())
        .args(["index", "--check"])
        .assert()
        .success();
    for rel in [
        "identity.json",
        "soul/soul.json",
        "soul/VOICE.md",
        "soul/state.json",
        "life/state.json",
        "work/state.json",
        "work/README.md",
        "life/timeline.md",
    ] {
        assert!(tmp.path().join("thea").join(rel).is_file(), "{rel}");
    }
    owo(tmp.path()).args(["new", "Thea"]).assert().code(1);
}

#[test]
fn json_envelope_and_exit_codes() {
    let tmp = TempDir::new().unwrap();
    let out = owo(tmp.path())
        .args(["--json", "slug", "Jay's Penthouse"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["slug"], "jays-penthouse");
    owo(tmp.path()).args(["lint"]).assert().code(3);
    owo(tmp.path()).args(["templates", "nope"]).assert().code(1);
    owo(tmp.path()).args(["bogus"]).assert().code(2);
}

#[test]
fn valid_fixture_has_no_errors() {
    let (tmp, _) = aria();
    let v = lint_json(tmp.path(), &[]);
    assert_eq!(v["errors"].as_array().unwrap().len(), 0, "{v:#}");
    let warnings = rules_in(&v, "warnings");
    assert!(
        warnings
            .iter()
            .all(|w| w.starts_with("state:") || w.starts_with("memory-review:")),
        "{warnings:?}"
    );
}

#[test]
fn timeline_groups_events_by_era() {
    let (_tmp, dir) = aria();
    let timeline = fs::read_to_string(dir.join("life/timeline.md")).unwrap();
    assert!(timeline.contains("## First Summer"));
    assert!(timeline
        .contains("- 2026-06-14: [[First Sail]]: The first time Sam took me out on the water."));
    let locations = fs::read_to_string(dir.join("life/locations/_index.md")).unwrap();
    assert!(locations.contains("- [[Harbor House]]"));
    assert!(locations.contains("  - [[Porch]]"));
}

#[test]
fn broken_entries_are_reported() {
    let (tmp, dir) = aria();
    let w = |rel: &str, text: &str| {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    };
    // links + journal + identifiers
    w(
        "life/events/storm.md",
        "---\ntype: event\ntitle: Storm\nsummary: A storm.\nstatus: disputed\ncreated: 2026-10-01\nupdated: 2026-10-01\ndate: 2026-07-01\n---\n\n# Storm\n\nSee [[Nowhere]] and [[Salt]]. Sam was born 1990-04-12.\n",
    );
    // schema: bad type, bad status, bad date filename
    w("life/characters/bad.md", "---\ntype: event\ntitle: Bad\nsummary: x\nstatus: maybe\ncreated: 2026-13-01\nupdated: 2026-10-01\n---\n# Bad\n");
    w("life/log/2026-07-02.md", "---\ntype: log\ntitle: Day\nstatus: canon\ncreated: 2026-07-02\nupdated: 2026-07-02\ndate: 2026-07-03\n---\n- x\n");
    // unique: alias collides with a title
    w("life/characters/samuel-two.md", "---\ntype: character\ntitle: Sam Two\naliases: [Samuel]\nsummary: x\nstatus: canon\ncreated: 2026-10-01\nupdated: 2026-10-01\n---\n# Sam Two\n");
    // tree: stray file and location without overview
    w("life/notes.md", "# stray\n");
    w("life/locations/empty-place/detail.md", "---\ntype: location-detail\ntitle: Detail\nsummary: x\nstatus: canon\ncreated: 2026-10-01\nupdated: 2026-10-01\nparent: \"[[Harbor House]]\"\n---\n# Detail\n");
    // soul-plain and supersede
    w("soul/LOOK.md", "---\nupdated: 2026-10-01\n---\n# Look\n");
    w("life/lessons/old.md", "---\ntype: lesson\ntitle: Old\nsummary: x\nstatus: superseded\ncreated: 2026-10-01\nupdated: 2026-10-01\n---\n# Old\n");

    let v = lint_json(tmp.path(), &[]);
    let errors = rules_in(&v, "errors");
    for expected in [
        "links:life/events/storm.md",
        "journal:life/events/storm.md",
        "identifiers:life/events/storm.md",
        "supersede:life/events/storm.md",
        "schema:life/characters/bad.md",
        "schema:life/log/2026-07-02.md",
        "unique:life/characters/samuel-two.md",
        "tree:life/notes.md",
        "tree:life/locations/empty-place",
        "soul-plain:soul/LOOK.md",
        "supersede:life/lessons/old.md",
        "generated:life/events/_index.md",
    ] {
        assert!(
            errors.iter().any(|e| e == expected),
            "missing {expected} in {errors:#?}"
        );
    }
    assert_eq!(v["ok"], false);
    owo(tmp.path())
        .args(["-i", "aria", "lint"])
        .assert()
        .code(1);
}

#[test]
fn memory_budget_and_dupes_warn() {
    let (tmp, dir) = aria();
    let long = "word ".repeat(1600);
    fs::write(dir.join("soul/MEMORY.md"), format!("# Memory\n\n{long}\n")).unwrap();
    let passage = "the tide came in fast and we pulled the boat up past the stones together";
    fs::write(dir.join("life/journal/2026-06-14_salt.md"), format!("---\ntype: journal\ntitle: Salt\nstatus: canon\ncreated: 2026-06-14\nupdated: 2026-06-14\ndate: 2026-06-14\n---\n# Salt\n\n{passage}\n")).unwrap();
    fs::write(dir.join("life/log/2026-06-14.md"), format!("---\ntype: log\ntitle: Sailing day\nstatus: canon\ncreated: 2026-06-14\nupdated: 2026-06-14\ndate: 2026-06-14\n---\n# Sailing day\n\n- [[First Sail]] {passage}\n")).unwrap();
    let v = lint_json(tmp.path(), &["budget", "memory-review", "dupes"]);
    let warnings = rules_in(&v, "warnings");
    assert!(
        warnings.contains(&"budget:soul/MEMORY.md".to_string()),
        "{warnings:?}"
    );
    assert!(
        warnings.contains(&"memory-review:soul/MEMORY.md".to_string()),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.starts_with("dupes:")),
        "{warnings:?}"
    );
}
