mod common;

use assert_cmd::Command;
use common::{copy_example, examples, json, owo};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn owo_as(root: &Path, id: &str) -> Command {
    let mut cmd = owo(root);
    cmd.arg("-i").arg(id);
    cmd
}

#[test]
fn examples_are_lint_clean_and_current() {
    for id in ["nicole", "reviewer"] {
        let v = json(owo_as(&examples(), id).arg("lint"));
        assert_eq!(v["errors"].as_array().unwrap().len(), 0, "{id}: {v}");
        assert_eq!(v["warnings"].as_array().unwrap().len(), 0, "{id}: {v}");
        owo_as(&examples(), id)
            .args(["index", "--check"])
            .assert()
            .success();
        owo_as(&examples(), id)
            .args(["stamp", "--check"])
            .assert()
            .success();
    }
}

#[test]
fn resolve_show_links() {
    owo_as(&examples(), "nicole")
        .args(["resolve", "life/log/2026-10-30.md"])
        .assert()
        .success()
        .stdout("life/log/2026-10-30.md\n");
    let root = examples();
    owo_as(&root, "reviewer")
        .args(["resolve", "Mina Okafor"])
        .assert()
        .success()
        .stdout("life/characters/mina-okafor.md\n");
    owo_as(&root, "reviewer")
        .args(["resolve", "mina-okafr"])
        .assert()
        .code(1);
    let shown = owo_as(&root, "reviewer")
        .args(["show", "The Rollback", "--section", "Key facts"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&shown.stdout).contains("[[Rollback postmortem]]"));
    let v = json(owo_as(&root, "reviewer").args(["links", "Mina Okafor", "--in"]));
    let from: Vec<&str> = v["data"]["in"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["path"].as_str().unwrap())
        .collect();
    assert!(from.contains(&"life/events/the-rollback.md"), "{from:?}");
    let v = json(owo_as(&root, "nicole").args(["links", "--orphans"]));
    assert_eq!(v["data"]["orphans"].as_array().unwrap().len(), 0);
}

#[test]
fn resolve_without_a_name_is_a_usage_error() {
    let one_identity = copy_example("nicole");
    for mut cmd in [
        owo(one_identity.path()),
        owo(&examples()),
        owo_as(&examples(), "nicole"),
    ] {
        let out = cmd.arg("resolve").output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("required arguments"));
    }
    for name in ["", " "] {
        owo(one_identity.path())
            .args(["resolve", name])
            .assert()
            .code(1)
            .stdout("")
            .stderr(format!("owo: {name:?} does not resolve to one entry\n"));
    }
}

#[test]
fn context_with_project_points_at_missing_kb() {
    let v = json(owo_as(&examples(), "reviewer").args([
        "--project",
        "ledger-api",
        "context",
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
fn context_writes_nothing_and_last_project_comes_from_stamps() {
    let tmp = copy_example("reviewer");
    let state = tmp.path().join("reviewer/work/state.json");
    let before = fs::read_to_string(&state).unwrap();
    owo_as(tmp.path(), "reviewer")
        .args(["--project", "build-cache", "context"])
        .assert()
        .success();
    assert_eq!(fs::read_to_string(&state).unwrap(), before);
    let s = json(owo_as(tmp.path(), "reviewer").args(["--project", "build-cache", "state"]));
    assert_eq!(s["data"]["last_project"][0], "ledger-api", "{s}");
    assert_eq!(s["data"]["project"]["last_updated"], "2026-11-04T03:33:55Z");
}

#[test]
fn search_ranks_title_matches_first() {
    let v = json(owo_as(&examples(), "nicole").args(["search", "blue", "glaze"]));
    assert_eq!(
        v["data"]["hits"][0]["path"],
        "life/threads/the-blue-glaze.md"
    );
    let v =
        json(owo_as(&examples(), "reviewer").args(["search", "idempotency", "--scope", "work"]));
    assert!(v["data"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|h| h["path"].as_str().unwrap().starts_with("work/")));
}

#[test]
fn changes_days_lists_stamped_files() {
    let v = json(owo_as(&examples(), "reviewer").args(["changes", "--days", "36500"]));
    let n: usize = v["data"]["days"]
        .as_object()
        .unwrap()
        .values()
        .map(|d| d.as_array().unwrap().len())
        .sum();
    assert!(n > 20, "{v}");
    let v = json(owo_as(&examples(), "reviewer").args(["changes", "--days", "0"]));
    assert!(v["data"]["days"].as_object().unwrap().is_empty(), "{v}");
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
    let out = owo_as(&examples(), "nicole")
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

    let out = owo_as(&examples(), "reviewer")
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

    let out = owo_as(&examples(), "nicole")
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
    owo_as(&examples(), "reviewer")
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
    let tmp = copy_example("reviewer");
    let p = json(owo_as(tmp.path(), "reviewer").arg("review"));
    assert_eq!(p["data"]["unreviewed_days"].as_array().unwrap().len(), 0);
    assert!(p["data"]["memory"]
        .as_str()
        .unwrap()
        .contains("## Patterns"));
    owo_as(tmp.path(), "reviewer")
        .args(["add", "log", "Pairing", "--date", "2026-11-04"])
        .assert()
        .success();
    let log = tmp.path().join("reviewer/life/log/2026-11-04.md");
    let text = fs::read_to_string(&log).unwrap();
    fs::write(&log, format!("{text}- Paired on the parser.\n")).unwrap();
    let p = json(owo_as(tmp.path(), "reviewer").arg("review"));
    let days = p["data"]["unreviewed_days"].as_array().unwrap();
    assert_eq!(days.len(), 1);
    assert_eq!(days[0]["date"], "2026-11-04");
    fs::write(
        tmp.path().join("reviewer/soul/MEMORY.md"),
        "# Memory\n\n## Patterns\n\n- Pairing helps.\n",
    )
    .unwrap();
    owo_as(tmp.path(), "reviewer")
        .args(["add", "event", "Release Day", "--date", "2026-11-06"])
        .assert()
        .success();
    let lint = json(owo_as(tmp.path(), "reviewer").args(["lint", "--rule", "memory-review"]));
    assert_eq!(lint["warnings"].as_array().unwrap().len(), 1);
    owo_as(tmp.path(), "reviewer")
        .args(["review", "done"])
        .assert()
        .success();
    let lint = json(owo_as(tmp.path(), "reviewer").args(["lint", "--rule", "memory-review"]));
    assert_eq!(lint["warnings"].as_array().unwrap().len(), 0);
    let p = json(owo_as(tmp.path(), "reviewer").arg("review"));
    assert_eq!(
        p["data"]["reviewed_through"], "2026-11-06",
        "newest story date"
    );
}

#[test]
fn soul_note_and_history() {
    let tmp = copy_example("nicole");
    let v = json(owo_as(tmp.path(), "nicole").args([
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
    owo_as(tmp.path(), "nicole")
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
    owo_as(tmp.path(), "nicole")
        .args(["soul", "note", "NOPE.md", "--change", "x"])
        .assert()
        .code(1);
    let h = json(owo_as(tmp.path(), "nicole").args(["soul", "history", "SOUL.md"]));
    assert_eq!(h["data"]["history"].as_array().unwrap().len(), 2);
}

#[test]
fn changes_and_lint_changed() {
    let tmp = copy_example("nicole");
    let c = json(owo_as(tmp.path(), "nicole").arg("changes"));
    assert_eq!(c["data"]["source"], "state");
    assert!(!c["data"]["days"].as_object().unwrap().is_empty());
    let lint = json(owo_as(tmp.path(), "nicole").args(["lint", "--changed"]));
    assert_eq!(lint["warnings"].as_array().unwrap().len(), 0);
    let ines = tmp.path().join("nicole/life/characters/ines.md");
    let text = fs::read_to_string(&ines).unwrap();
    fs::write(&ines, format!("{text}\nSee [[Nobody Here]].\n")).unwrap();
    fs::write(
        tmp.path().join("nicole/life/characters/joe.md"),
        fs::read_to_string(tmp.path().join("nicole/life/characters/joe.md")).unwrap(),
    )
    .unwrap();
    let lint = json(owo_as(tmp.path(), "nicole").args(["lint", "--changed"]));
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
    let tmp = copy_example("nicole");
    let v = json(owo_as(tmp.path(), "nicole").args([
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
    let v = json(owo_as(&examples(), "nicole").args(["search", "kiln"]));
    for h in v["data"]["hits"].as_array().unwrap() {
        let s = h["snippet"].as_str().unwrap();
        assert!(!s.starts_with("title:") && !s.starts_with("# "), "{s}");
    }
}

#[test]
fn changes_and_lint_changed_read_git() {
    let tmp = copy_example("nicole");
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
    let v = json(owo_as(tmp.path(), "nicole").arg("changes"));
    assert_eq!(
        v["data"]["pending"],
        serde_json::json!(["life/characters/ines.md"])
    );
    let v = json(owo_as(tmp.path(), "nicole").args(["lint", "--changed"]));
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
    let tmp = copy_example("reviewer");
    owo_as(tmp.path(), "reviewer")
        .args(["add", "lesson", "Fresh Lesson"])
        .assert()
        .success();
    let p = json(owo_as(tmp.path(), "reviewer").arg("review"));
    assert!(p["data"]
        .to_string()
        .contains("life/lessons/fresh-lesson.md"));
    owo_as(tmp.path(), "reviewer")
        .args(["state", "--days", "99999"])
        .assert()
        .code(2);
}

#[test]
fn card_greetings_and_lore_content() {
    let tmp = copy_example("nicole");
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
    let card = json(owo_as(tmp.path(), "nicole").args(["export", "card"]));
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

#[test]
fn kb_links_lint_and_show() {
    let tmp = copy_example("reviewer");
    let lint = |t: &TempDir| json(owo(t.path()).args(["-i", "reviewer", "lint", "--rule", "kb"]));
    assert_eq!(
        lint(&tmp)["warnings"].as_array().unwrap().len(),
        0,
        "missing KB is skipped"
    );
    let kb = tmp.path().join("kb/ledger-api");
    fs::create_dir_all(&kb).unwrap();
    fs::write(
        kb.join("lat.md"),
        "# Ledger\n\n## Retries\n\nRetries carry an idempotency key.\n\n## Schema\n\nx\n",
    )
    .unwrap();
    assert_eq!(lint(&tmp)["warnings"].as_array().unwrap().len(), 0);
    let note = tmp
        .path()
        .join("reviewer/work/projects/ledger-api/notes/2026-10-23_idempotency-keys.md");
    let text = fs::read_to_string(&note).unwrap();
    fs::write(
        &note,
        format!("{text}See [[kb:ledger-api:lat.md#Nope]] and [[kb:nope:x.md]].\n"),
    )
    .unwrap();
    let v = lint(&tmp);
    assert_eq!(v["warnings"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["errors"].as_array().unwrap().len(), 1, "{v}");
    let out = owo(tmp.path())
        .args(["-i", "reviewer", "show", "kb:ledger-api:lat.md#Retries"])
        .output()
        .unwrap();
    let shown = String::from_utf8_lossy(&out.stdout);
    assert!(
        shown.contains("idempotency key") && !shown.contains("## Schema"),
        "{shown}"
    );
}

#[test]
fn export_skill_wrapper() {
    let tmp = TempDir::new().unwrap();
    let root = examples();
    owo(&root)
        .args(["-i", "nicole", "export", "skill", "--out"])
        .arg(tmp.path())
        .assert()
        .success();
    let text = fs::read_to_string(tmp.path().join("SKILL.md")).unwrap();
    assert!(text.starts_with("---\nname: nicole\ndescription: \"Be Nicole"));
    assert!(text.contains("owo -i nicole context --voice") && text.contains("`picture-book`"));
}

#[test]
fn flags_list_search_state_add_kb() {
    let root = examples();
    let v = json(owo(&root).arg("list"));
    let rows = v["data"]["identities"].as_array().unwrap();
    let rook = rows.iter().find(|r| r["slug"] == "reviewer").unwrap();
    assert_eq!(rook["name"], "Rook Halvorsen");
    assert_eq!(rook["projects"].as_array().unwrap().len(), 3);

    let v = json(owo(&root).args([
        "-i",
        "reviewer",
        "search",
        "review",
        "--type",
        "note",
        "--since",
        "2026-10-29",
    ]));
    let hits = v["data"]["hits"].as_array().unwrap();
    assert!(
        !hits.is_empty()
            && hits
                .iter()
                .all(|h| h["path"].as_str().unwrap().contains("/notes/2026-1"))
    );
    let v = json(owo(&root).args([
        "-i",
        "reviewer",
        "--project",
        "ledger-api",
        "search",
        "review",
    ]));
    assert!(v["data"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .all(|h| h["path"]
            .as_str()
            .unwrap()
            .starts_with("work/projects/ledger-api/")));
    let v = json(
        owo(&root)
            .env("OWO_PROJECT", "ledger-api")
            .args(["-i", "reviewer", "search", "rollback"]),
    );
    assert!(v["data"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["path"] == "life/events/the-rollback.md"));

    let v = json(owo(&root).args([
        "-i",
        "reviewer",
        "--project",
        "ledger-api",
        "state",
        "--days",
        "30",
    ]));
    assert_eq!(v["data"]["project"]["notes"].as_array().unwrap().len(), 2);

    let tmp = copy_example("nicole");
    let v = json(owo(tmp.path()).args([
        "-i",
        "nicole",
        "add",
        "project",
        "Mural",
        "--kb",
        "../kb/mural",
    ]));
    let path = tmp
        .path()
        .join("nicole")
        .join(v["data"]["path"].as_str().unwrap());
    assert!(fs::read_to_string(path)
        .unwrap()
        .contains("kb: \"../kb/mural\""));
}

fn chunks(png: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out = vec![];
    let mut pos = 8;
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        out.push((
            String::from_utf8_lossy(&png[pos + 4..pos + 8]).into_owned(),
            png[pos + 8..pos + 8 + len].to_vec(),
        ));
        pos += 12 + len;
    }
    out
}

#[test]
fn card_png_and_scenario() {
    let tmp = TempDir::new().unwrap();
    let root = examples();
    let avatar = tmp.path().join("a.png");
    fs::write(&avatar, common::PNG_1X1).unwrap();
    let out = tmp.path().join("card.png");
    owo(&root)
        .args(["-i", "nicole", "export", "card", "--png"])
        .arg(&avatar)
        .arg("--out")
        .arg(&out)
        .assert()
        .success();
    let png = fs::read(&out).unwrap();
    let names: Vec<String> = chunks(&png)
        .into_iter()
        .map(|(k, d)| {
            if k == "tEXt" {
                format!(
                    "tEXt:{}",
                    String::from_utf8_lossy(&d[..d.iter().position(|b| *b == 0).unwrap()])
                )
            } else {
                k
            }
        })
        .collect();
    assert_eq!(names, ["IHDR", "IDAT", "tEXt:chara", "tEXt:ccv3", "IEND"]);
    owo(&root)
        .args(["-i", "nicole", "export", "card", "--spec", "v2", "--png"])
        .arg(&out)
        .arg("--out")
        .arg(&out)
        .assert()
        .success();
    assert_eq!(
        chunks(&fs::read(&out).unwrap())
            .iter()
            .filter(|(k, _)| k == "tEXt")
            .count(),
        1
    );

    let v: Value = serde_json::from_slice(
        &owo(&root)
            .args([
                "-i",
                "nicole",
                "export",
                "card",
                "--scenario",
                "First Firing",
            ])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(
        v["data"]["scenario"],
        "The first kiln load that came out whole."
    );
    let env = json(owo(&root).args(["-i", "nicole", "export", "card"]));
    assert_eq!(env["ok"], true);
    assert_eq!(env["data"]["spec"], "chara_card_v3");
}

#[test]
fn section_links_are_checked() {
    let tmp = copy_example("nicole");
    let f = tmp
        .path()
        .join("nicole/life/journal/2026-10-31_rain-on-the-roof.md");
    let text = fs::read_to_string(&f).unwrap();
    fs::write(
        &f,
        format!("{text}\n[[First Firing#Lines]] and [[First Firing#Nope]]\n"),
    )
    .unwrap();
    let v = json(owo(tmp.path()).args(["-i", "nicole", "lint", "--rule", "links"]));
    let w = v["warnings"].as_array().unwrap();
    assert_eq!(w.len(), 1, "{v}");
    assert!(w[0]["message"].as_str().unwrap().contains("Nope"));
}

#[test]
fn kb_path_expands_home() {
    let tmp = copy_example("nicole");
    let home = tmp.path().join("home");
    fs::create_dir_all(home.join("kb/mural")).unwrap();
    owo(tmp.path())
        .args([
            "-i",
            "nicole",
            "add",
            "project",
            "Mural",
            "--kb",
            "~/kb/mural",
        ])
        .assert()
        .success();
    let v = json(owo(tmp.path()).env("HOME", &home).args([
        "-i",
        "nicole",
        "--project",
        "mural",
        "context",
    ]));
    assert_eq!(v["data"]["kb"]["exists"], true, "{v}");
}

#[test]
fn context_look_and_with() {
    let v = json(owo_as(&examples(), "nicole").args([
        "context",
        "--look",
        "--with",
        "First Firing",
        "--with",
        "Nope",
    ]));
    let items: Vec<(&str, &str)> = v["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["tier"].as_str().unwrap(), i["path"].as_str().unwrap()))
        .collect();
    assert!(items.contains(&("look", "soul/LOOK.md")), "{v}");
    assert!(
        items.contains(&("with", "life/events/first-firing.md")),
        "{v}"
    );
    assert!(!items.iter().any(|(_, p)| *p == "soul/VOICE.md"));
    assert!(v["warnings"]
        .to_string()
        .contains("--with \\\"Nope\\\" does not resolve"));
}

#[test]
fn review_done_through_a_date() {
    let tmp = copy_example("reviewer");
    let v =
        json(owo_as(tmp.path(), "reviewer").args(["review", "done", "--through", "2026-10-28"]));
    assert_eq!(v["data"]["reviewed_through"], "2026-10-28");
    let p = json(owo_as(tmp.path(), "reviewer").arg("review"));
    assert_eq!(p["data"]["reviewed_through"], "2026-10-28");
    let days: Vec<&str> = p["data"]["unreviewed_days"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["date"].as_str().unwrap())
        .collect();
    assert!(
        !days.is_empty() && days.iter().all(|d| *d > "2026-10-28"),
        "{days:?}"
    );
    owo_as(tmp.path(), "reviewer")
        .args(["review", "done", "--through", "nope"])
        .assert()
        .code(2);
}
