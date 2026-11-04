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
    owo(tmp.path())
        .args(["-i", "nosuch", "lint"])
        .assert()
        .code(3);
    owo(tmp.path())
        .args(["templates", "show", "nope"])
        .assert()
        .code(1);
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
    fs::write(
        dir.join("life/events/regatta.md"),
        "---\ntype: event\ntitle: Regatta\nsummary: Boats.\nstatus: canon\ncreated: 2026-07-01\nupdated: 2026-07-01\ndate: 2026-07-01\nera: \"[[first-summer|that summer]]\"\n---\n\n# Regatta\n",
    )
    .unwrap();
    owo(dir.parent().unwrap())
        .args(["-i", "aria", "index"])
        .assert()
        .success();
    let timeline = fs::read_to_string(dir.join("life/timeline.md")).unwrap();
    let era = timeline.split("## First Summer").nth(1).unwrap();
    assert!(
        era.split("\n## ").next().unwrap().contains("[[Regatta]]"),
        "{timeline}"
    );
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

#[test]
fn add_creates_entries_at_deterministic_paths() {
    let tmp = TempDir::new().unwrap();
    owo(tmp.path()).args(["new", "Thea"]).assert().success();
    let add = |args: &[&str]| {
        owo(tmp.path())
            .args(["-i", "thea", "add"])
            .args(args)
            .output()
            .unwrap()
    };
    assert_eq!(
        String::from_utf8(
            add(&["character", "Mrs.", "Petrova", "--alias", "the neighbour"]).stdout
        )
        .unwrap()
        .trim(),
        "life/characters/mrs-petrova.md"
    );
    add(&["location", "Joe's Apartment"]);
    let detail = add(&["location-detail", "Balcony", "--parent", "Joe's Apartment"]);
    assert_eq!(
        String::from_utf8(detail.stdout).unwrap().trim(),
        "life/locations/joes-apartment/balcony.md"
    );
    add(&["event", "The Café Night", "--date", "2026-10-25"]);
    add(&["journal", "Two beds", "--date", "2026-10-28"]);
    add(&["project", "API Server"]);
    let note = owo(tmp.path())
        .args([
            "-i",
            "thea",
            "--project",
            "api-server",
            "add",
            "note",
            "Release steps",
            "--date",
            "2026-11-12",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(note.stdout).unwrap().trim(),
        "work/projects/api-server/notes/2026-11-12_release-steps.md"
    );
    let dir = tmp.path().join("thea");
    for rel in [
        "life/locations/joes-apartment/overview.md",
        "life/events/the-cafe-night.md",
        "life/journal/2026-10-28_two-beds.md",
        "work/projects/api-server/project.md",
    ] {
        assert!(dir.join(rel).is_file(), "{rel}");
    }
    let detail_text =
        fs::read_to_string(dir.join("life/locations/joes-apartment/balcony.md")).unwrap();
    assert!(detail_text.contains("parent: \"[[Joe's Apartment]]\""));
    assert!(
        fs::read_to_string(dir.join("life/characters/mrs-petrova.md"))
            .unwrap()
            .contains("aliases:\n  - \"the neighbour\"")
    );
    owo(tmp.path())
        .args(["-i", "thea", "add", "event", "The Café Night"])
        .assert()
        .code(1);
    owo(tmp.path())
        .args(["-i", "thea", "add", "log", "--date", "2026-10-25"])
        .assert()
        .success();
    owo(tmp.path())
        .args(["-i", "thea", "add", "log", "--date", "2026-10-25"])
        .assert()
        .success();

    owo(tmp.path())
        .args(["-i", "thea", "index"])
        .assert()
        .success();
    let v: Value = serde_json::from_slice(
        &owo(tmp.path())
            .args([
                "-i", "thea", "lint", "--json", "--rule", "schema", "--rule", "links", "--rule",
                "tree",
            ])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(v["errors"].as_array().unwrap().len(), 0, "{v:#}");
}

#[test]
fn stamp_tracks_changes_by_hash() {
    let (tmp, dir) = aria();
    owo(tmp.path())
        .args(["-i", "aria", "stamp", "--check"])
        .assert()
        .code(1);
    owo(tmp.path())
        .args(["-i", "aria", "stamp"])
        .assert()
        .success();
    owo(tmp.path())
        .args(["-i", "aria", "stamp", "--check"])
        .assert()
        .success();
    let v = lint_json(tmp.path(), &["state"]);
    assert_eq!(v["warnings"].as_array().unwrap().len(), 0, "{v:#}");

    let soul_version = |d: &Path| {
        serde_json::from_str::<Value>(&fs::read_to_string(d.join("soul/soul.json")).unwrap())
            .unwrap()["version"]
            .clone()
    };
    let before = soul_version(&dir);
    fs::write(dir.join("soul/SOUL.md"), "# Soul\n\nI am Aria. I sail.\n").unwrap();
    let sam = dir.join("life/characters/sam.md");
    fs::write(
        &sam,
        fs::read_to_string(&sam)
            .unwrap()
            .replace("sails with me", "sails with me most weekends"),
    )
    .unwrap();
    let out: Value = serde_json::from_slice(
        &owo(tmp.path())
            .args(["-i", "aria", "--json", "stamp"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let changed: Vec<&str> = out["data"]["changed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(changed, vec!["life/characters/sam.md", "soul/SOUL.md"]);
    assert_ne!(soul_version(&dir), before);
    assert!(!fs::read_to_string(&sam)
        .unwrap()
        .contains("updated: 2026-10-02"));
    owo(tmp.path())
        .args(["-i", "aria", "stamp", "--check"])
        .assert()
        .success();
}

#[test]
fn templates_list_show_and_override() {
    let tmp = TempDir::new().unwrap();
    let tpl = tmp.path().join(".templates");
    fs::create_dir_all(tpl.join("soul")).unwrap();
    let character = "---\ntype: character\ntitle: \"{{title_yaml}}\"\naliases: []\nsummary: \"\"\nstatus: canon\ncreated: {{today}}\nupdated: {{today}}\nsources: []\n---\n\n# {{title}}\n\nCustom.\n";
    fs::write(tpl.join("character.md"), character).unwrap();
    fs::write(tpl.join("soul/IDENTITY.md"), "# Custom identity\n").unwrap();
    fs::write(tpl.join("README.md"), "notes about this folder\n").unwrap();

    let out = owo(tmp.path())
        .args(["--json", "templates", "list"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let list = v["data"]["templates"].as_array().unwrap();
    let row = |n: &str| list.iter().find(|r| r["name"] == n).unwrap().clone();
    assert!(row("character")["override"].is_string());
    assert!(row("soul/IDENTITY.md")["override"].is_string());
    assert!(row("work/README.md")["override"].is_null());
    owo(tmp.path())
        .args(["templates", "show", "character"])
        .assert()
        .success()
        .stdout(character);

    owo(tmp.path()).args(["new", "Zed"]).assert().success();
    let dir = tmp.path().join("zed");
    assert_eq!(
        fs::read_to_string(dir.join("soul/IDENTITY.md")).unwrap(),
        "# Custom identity\n"
    );
    assert!(!fs::read_to_string(dir.join("work/README.md"))
        .unwrap()
        .contains("notes about this folder"));
    owo(tmp.path())
        .args(["-i", "zed", "add", "character", "Bo"])
        .assert()
        .success();
    assert!(fs::read_to_string(dir.join("life/characters/bo.md"))
        .unwrap()
        .contains("Custom."));
}

#[test]
fn add_defaults_and_escapes() {
    let tmp = TempDir::new().unwrap();
    owo(tmp.path()).args(["new", "Zed"]).assert().success();
    let add = |args: &[&str]| {
        owo(tmp.path())
            .args(["-i", "zed", "add"])
            .args(args)
            .assert()
    };
    add(&["log", "--date", "2026-01-01"]).success();
    add(&["log", "--date", "2026-01-02"]).success();
    add(&["project", "API Server"]).success();
    add(&[
        "character",
        "Q",
        "--alias",
        r"back\slash",
        "--alias",
        "say \"hi\"",
    ])
    .success();
    add(&["character"]).code(2);
    let lint = owo(tmp.path())
        .args([
            "--json", "-i", "zed", "lint", "--rule", "unique", "--rule", "schema",
        ])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&lint.stdout).unwrap();
    assert_eq!(v["errors"].as_array().unwrap().len(), 0, "{v}");
    owo(tmp.path())
        .args(["-i", "zed", "resolve", "api-server"])
        .assert()
        .success()
        .stdout("work/projects/api-server/project.md\n");
    owo(tmp.path())
        .args(["-i", "zed", "resolve", r"back\slash"])
        .assert()
        .success()
        .stdout("life/characters/q.md\n");
}

#[test]
fn identifiers_stay_under_their_section() {
    let (tmp, dir) = aria();
    assert_eq!(
        lint_json(tmp.path(), &["identifiers"])["errors"],
        Value::Array(vec![])
    );
    let sam = dir.join("life/characters/sam.md");
    let text = fs::read_to_string(&sam).unwrap();
    fs::write(
        &sam,
        text.replace("Sam sails with me.", "Sam (1990-04-12) sails with me."),
    )
    .unwrap();
    let v = lint_json(tmp.path(), &["identifiers"]);
    assert_eq!(
        rules_in(&v, "errors"),
        vec!["identifiers:life/characters/sam.md"]
    );
}
