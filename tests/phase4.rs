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
        "life/locations/the-lighthouse/overview.md",
        "life/locations/the-lighthouse/lamp-room.md",
        "life/locations/harbour/overview.md",
        "life/threads/paint-the-door.md",
    ] {
        assert!(out.join(f).is_file(), "{f}");
    }
    assert!(tmp
        .path()
        .join("sources/lumen/2026-01-01_session.md")
        .is_file());
    let skipped = v["data"]["skipped"].to_string();
    assert!(skipped.contains("changelog.md") && skipped.contains("notes.txt"));
    let room = fs::read_to_string(out.join("life/locations/the-lighthouse/lamp-room.md")).unwrap();
    assert!(
        room.contains("type: location-detail") && room.contains("parent: '[[The Lighthouse]]'"),
        "{room}"
    );
    assert!(!skipped.contains("locations"), "{skipped}");
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    owo(&root)
        .args(["-i", "thea", "export", "skill", "--out"])
        .arg(tmp.path())
        .assert()
        .success();
    let text = fs::read_to_string(tmp.path().join("SKILL.md")).unwrap();
    assert!(text.starts_with("---\nname: thea\ndescription: \"Be Thea"));
    assert!(
        text.contains("owo -i thea context --format text --voice")
            && text.contains("`picture-book`")
    );
}

#[test]
fn flags_list_search_state_add_kb() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
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

    let tmp = copy_example("thea");
    let v = json(owo(tmp.path()).args([
        "-i",
        "thea",
        "add",
        "project",
        "Mural",
        "--kb",
        "../kb/mural",
    ]));
    let path = tmp
        .path()
        .join("thea")
        .join(v["data"]["path"].as_str().unwrap());
    assert!(fs::read_to_string(path)
        .unwrap()
        .contains("kb: \"../kb/mural\""));
}

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let avatar = tmp.path().join("a.png");
    fs::write(&avatar, PNG_1X1).unwrap();
    let out = tmp.path().join("card.png");
    owo(&root)
        .args(["-i", "thea", "export", "card", "--png"])
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
    // embedding again replaces the chunks
    owo(&root)
        .args(["-i", "thea", "export", "card", "--spec", "v2", "--png"])
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
            .args(["-i", "thea", "export", "card", "--scenario", "First Firing"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(
        v["data"]["scenario"],
        "The first kiln load that came out whole."
    );
}

#[test]
fn section_links_are_checked() {
    let tmp = copy_example("thea");
    let f = tmp
        .path()
        .join("thea/life/journal/2026-10-31_rain-on-the-roof.md");
    let text = fs::read_to_string(&f).unwrap();
    fs::write(
        &f,
        format!("{text}\n[[First Firing#Lines]] and [[First Firing#Nope]]\n"),
    )
    .unwrap();
    let v = json(owo(tmp.path()).args(["-i", "thea", "lint", "--rule", "links"]));
    let w = v["warnings"].as_array().unwrap();
    assert_eq!(w.len(), 1, "{v}");
    assert!(w[0]["message"].as_str().unwrap().contains("Nope"));
}
