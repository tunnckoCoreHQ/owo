//! End-to-end test for `owo migrate v1`.

mod common;

use assert_cmd::Command;
use common::{json, owo};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

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
        "life/events/lantern-night.md",
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
        room.contains("type: location-detail") && room.contains("parent: \"[[The Lighthouse]]\""),
        "{room}"
    );
    assert!(!skipped.contains("locations"), "{skipped}");
    assert!(!skipped.contains("index.md") && !out.join("life/characters/index.md").exists());
    let storm = fs::read_to_string(out.join("life/events/the-storm.md")).unwrap();
    assert!(
        storm.contains("sources:\n  - lumen/2026-01-01_session.md")
            && !storm.contains("## Sources"),
        "{storm}"
    );
    assert!(fs::read_to_string(out.join("life/characters/ada.md"))
        .unwrap()
        .contains("status: canon"));
    assert!(
        skipped.contains("2026-01-02_extra.md (target life/log/2026-01-02.md exists"),
        "{skipped}"
    );
    assert_eq!(
        fs::read_to_string(out.join("life/log/2026-01-02.md"))
            .unwrap()
            .trim_end()
            .lines()
            .last(),
        Some("Calm.")
    );
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
