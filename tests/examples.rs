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
    for id in ["nicole", "reviewer"] {
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
    owo(&examples(), "nicole")
        .args(["resolve", "life/log/2026-10-30.md"])
        .assert()
        .success()
        .stdout("life/log/2026-10-30.md\n");
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
    let v = json(owo(&root, "nicole").args(["links", "--orphans"]));
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
    for entry in walkdir::WalkDir::new(examples().join("nicole")) {
        let entry = entry.unwrap();
        let target = tmp.path().join("nicole").join(
            entry
                .path()
                .strip_prefix(examples().join("nicole"))
                .unwrap(),
        );
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
    for _ in 0..2 {
        owo(tmp.path(), "nicole")
            .args(["--project", "picture-book", "context", "--format", "text"])
            .assert()
            .success();
    }
    let state: Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("nicole/work/state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["projects"]["picture-book"]["sessions"], 2);
    let s = json(owo(tmp.path(), "nicole").arg("state"));
    assert_eq!(s["data"]["last_project"][0], "picture-book");
}

#[test]
fn search_ranks_title_matches_first() {
    let v = json(owo(&examples(), "nicole").args(["search", "blue", "glaze"]));
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

fn copy(id: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let from = examples().join(id);
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

fn check_card(card: &Value, name: &str) {
    assert_eq!(card["spec"], "chara_card_v3");
    assert_eq!(card["spec_version"], "3.0");
    let d = &card["data"];
    assert_eq!(d["name"], name);
    for key in [
        "description",
        "personality",
        "scenario",
        "first_mes",
        "mes_example",
        "creator_notes",
        "system_prompt",
        "post_history_instructions",
        "creator",
        "character_version",
    ] {
        assert!(d[key].is_string(), "{key} should be a string");
    }
    for key in [
        "alternate_greetings",
        "tags",
        "group_only_greetings",
        "source",
        "assets",
    ] {
        assert!(d[key].is_array(), "{key} should be an array");
    }
    assert!(d["creation_date"].is_i64() && d["extensions"].is_object());
    assert!(d["mes_example"].as_str().unwrap().starts_with("<START>"));
    let entries = d["character_book"]["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["constant"] == true));
    for e in entries {
        assert!(e["keys"].as_array().unwrap().iter().all(Value::is_string));
        assert!(e["content"].is_string() && e["enabled"].is_boolean() && e["use_regex"] == false);
        assert!(e["insertion_order"].is_i64() && e["extensions"].is_object());
    }
}

#[test]
fn export_card_v3_shape() {
    let out = owo(&examples(), "nicole")
        .args(["export", "card"])
        .output()
        .unwrap();
    let card: Value = serde_json::from_slice(&out.stdout).unwrap();
    check_card(&card, "Nicole");
    assert_eq!(card["data"]["nickname"], "Nikki");
    assert_eq!(
        card["data"]["alternate_greetings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let keys: Vec<String> = card["data"]["character_book"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|e| {
            e["keys"]
                .as_array()
                .unwrap()
                .iter()
                .map(|k| k.as_str().unwrap().to_string())
        })
        .collect();
    assert!(
        keys.contains(&"Kiln Studio".to_string()) && !keys.contains(&"Picture Book".to_string())
    );

    let out = owo(&examples(), "reviewer")
        .args(["--project", "ledger-api", "export", "card"])
        .output()
        .unwrap();
    let card: Value = serde_json::from_slice(&out.stdout).unwrap();
    check_card(&card, "Rook Halvorsen");
    assert!(card["data"]["scenario"]
        .as_str()
        .unwrap()
        .contains("Payments ledger"));
    let names: Vec<&str> = card["data"]["character_book"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"Ledger API") && !names.contains(&"Build Cache"),
        "{names:?}"
    );

    let out = owo(&examples(), "nicole")
        .args(["export", "card", "--spec", "v2"])
        .output()
        .unwrap();
    let card: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(card["spec"], "chara_card_v2");
    assert!(card["data"].get("assets").is_none());
}

#[test]
fn export_soulspec_package() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("pkg");
    owo(&examples(), "reviewer")
        .args(["export", "soulspec", "--out"])
        .arg(&dir)
        .assert()
        .success();
    for f in [
        "soul.json",
        "SOUL.md",
        "IDENTITY.md",
        "STYLE.md",
        "MEMORY.md",
        "examples/good.md",
    ] {
        assert!(dir.join(f).is_file(), "{f}");
    }
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(dir.join("soul.json")).unwrap()).unwrap();
    assert_eq!(manifest["specVersion"], "0.5");
    assert_eq!(manifest["files"]["style"], "STYLE.md");
    assert!(fs::read_to_string(dir.join("IDENTITY.md"))
        .unwrap()
        .contains("## Look"));
}

#[test]
fn review_packet_then_done() {
    let tmp = copy("reviewer");
    let p = json(owo(tmp.path(), "reviewer").arg("review"));
    assert_eq!(p["data"]["unreviewed_days"].as_array().unwrap().len(), 0);
    assert!(p["data"]["memory"]
        .as_str()
        .unwrap()
        .contains("## Patterns"));
    owo(tmp.path(), "reviewer")
        .args(["add", "log", "Pairing", "--date", "2026-11-04"])
        .assert()
        .success();
    let log = tmp.path().join("reviewer/life/log/2026-11-04.md");
    let text = fs::read_to_string(&log).unwrap();
    fs::write(&log, format!("{text}- Paired on the parser.\n")).unwrap();
    let p = json(owo(tmp.path(), "reviewer").arg("review"));
    let days = p["data"]["unreviewed_days"].as_array().unwrap();
    assert_eq!(days.len(), 1);
    assert_eq!(days[0]["date"], "2026-11-04");
    fs::write(
        tmp.path().join("reviewer/soul/MEMORY.md"),
        "# Memory\n\n## Patterns\n\n- Pairing helps.\n",
    )
    .unwrap();
    owo(tmp.path(), "reviewer")
        .args(["add", "event", "Release Day", "--date", "2026-11-06"])
        .assert()
        .success();
    let lint = json(owo(tmp.path(), "reviewer").args(["lint", "--rule", "memory-review"]));
    assert_eq!(lint["warnings"].as_array().unwrap().len(), 1);
    owo(tmp.path(), "reviewer")
        .args(["review", "done"])
        .assert()
        .success();
    let lint = json(owo(tmp.path(), "reviewer").args(["lint", "--rule", "memory-review"]));
    assert_eq!(lint["warnings"].as_array().unwrap().len(), 0);
    let p = json(owo(tmp.path(), "reviewer").arg("review"));
    assert_eq!(
        p["data"]["reviewed_through"], "2026-11-06",
        "newest story date"
    );
}

#[test]
fn soul_note_and_history() {
    let tmp = copy("nicole");
    let v = json(owo(tmp.path(), "nicole").args([
        "soul",
        "note",
        "SOUL.md",
        "--change",
        "Waiting is part of the craft.",
        "--evidence",
        "[[Let It Dry]]",
    ]));
    assert_eq!(
        v["warnings"].as_array().unwrap().len(),
        1,
        "one non-defining link warns"
    );
    owo(tmp.path(), "nicole")
        .args([
            "soul",
            "note",
            "SOUL.md",
            "--change",
            "x",
            "--evidence",
            "Nowhere",
        ])
        .assert()
        .code(1);
    owo(tmp.path(), "nicole")
        .args(["soul", "note", "NOPE.md", "--change", "x"])
        .assert()
        .code(1);
    let h = json(owo(tmp.path(), "nicole").args(["soul", "history", "SOUL.md"]));
    assert_eq!(h["data"]["history"].as_array().unwrap().len(), 2);
}

#[test]
fn changes_and_lint_changed() {
    let tmp = copy("nicole");
    let c = json(owo(tmp.path(), "nicole").arg("changes"));
    assert_eq!(c["data"]["source"], "state");
    assert!(!c["data"]["days"].as_object().unwrap().is_empty());
    let lint = json(owo(tmp.path(), "nicole").args(["lint", "--changed"]));
    assert_eq!(lint["warnings"].as_array().unwrap().len(), 0);
    let ines = tmp.path().join("nicole/life/characters/ines.md");
    let text = fs::read_to_string(&ines).unwrap();
    fs::write(&ines, format!("{text}\nSee [[Nobody Here]].\n")).unwrap();
    fs::write(
        tmp.path().join("nicole/life/characters/joe.md"),
        fs::read_to_string(tmp.path().join("nicole/life/characters/joe.md")).unwrap(),
    )
    .unwrap();
    let lint = json(owo(tmp.path(), "nicole").args(["lint", "--changed"]));
    let errors = lint["errors"].as_array().unwrap();
    assert!(
        !errors.is_empty()
            && errors
                .iter()
                .all(|e| e["path"] == "life/characters/ines.md"),
        "{lint}"
    );
}

#[test]
fn soul_note_counts_distinct_evidence() {
    let tmp = copy("nicole");
    let v = json(owo(tmp.path(), "nicole").args([
        "soul",
        "note",
        "SOUL.md",
        "--change",
        "Waiting is part of the craft.",
        "--evidence",
        "[[Let It Dry]]",
        "--evidence",
        "Let It Dry",
    ]));
    assert_eq!(v["warnings"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["data"]["evidence"].as_array().unwrap().len(), 1);
}

#[test]
fn search_snippets_come_from_the_body() {
    let v = json(owo(&examples(), "nicole").args(["search", "kiln"]));
    for h in v["data"]["hits"].as_array().unwrap() {
        let s = h["snippet"].as_str().unwrap();
        assert!(!s.starts_with("title:") && !s.starts_with("# "), "{s}");
    }
}

#[test]
fn changes_and_lint_changed_read_git() {
    let tmp = copy("nicole");
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(tmp.path())
            .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    let ines = tmp.path().join("nicole/life/characters/ines.md");
    let text = fs::read_to_string(&ines).unwrap();
    fs::write(&ines, format!("{text}\nSee [[Nowhere]].\n")).unwrap();
    let v = json(owo(tmp.path(), "nicole").arg("changes"));
    assert_eq!(
        v["data"]["pending"],
        serde_json::json!(["life/characters/ines.md"])
    );
    let v = json(owo(tmp.path(), "nicole").args(["lint", "--changed"]));
    let errors = v["errors"].as_array().unwrap();
    assert!(
        !errors.is_empty()
            && errors
                .iter()
                .all(|e| e["path"] == "life/characters/ines.md"),
        "{v}"
    );
}

#[test]
fn review_lists_unstamped_files() {
    let tmp = copy("reviewer");
    owo(tmp.path(), "reviewer")
        .args(["add", "lesson", "Fresh Lesson"])
        .assert()
        .success();
    let p = json(owo(tmp.path(), "reviewer").arg("review"));
    assert!(p["data"]
        .to_string()
        .contains("life/lessons/fresh-lesson.md"));
    owo(tmp.path(), "reviewer")
        .args(["state", "--days", "99999"])
        .assert()
        .code(2);
}

#[test]
fn card_greetings_and_lore_content() {
    let tmp = copy("nicole");
    let dir = tmp.path().join("nicole");
    fs::write(
        dir.join("soul/examples/greetings.md"),
        "# Greetings\n\nOne greeting per section.\nKeep them short.\n\n## Morning\n\nHello there.\n",
    )
    .unwrap();
    let ines = dir.join("life/characters/ines.md");
    let text = fs::read_to_string(&ines).unwrap();
    fs::write(
        &ines,
        format!("{text}\n## Key facts\n\n- Fires on Fridays.\n"),
    )
    .unwrap();
    let card = json(owo(tmp.path(), "nicole").args(["export", "card"]));
    let d = &card["data"]["data"];
    assert_eq!(d["first_mes"], "Hello there.");
    assert_eq!(d["alternate_greetings"], serde_json::json!([]));
    let entry = d["character_book"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == "Ines")
        .unwrap();
    assert_eq!(
        entry["content"],
        "Ceramicist who shares the studio and the kiln.\n\n- Fires on Fridays."
    );
}
