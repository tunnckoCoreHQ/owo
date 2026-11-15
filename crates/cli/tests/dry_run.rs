//! `--dry-run`: each command that writes files reports the files it would write and writes
//! none, and the real run then writes those files.

mod common;

use common::{copy_tree, examples, json, owo};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Every path under `dir` with its bytes (None for a folder).
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .map(|e| {
            let e = e.unwrap();
            let bytes = e.file_type().is_file().then(|| fs::read(e.path()).unwrap());
            (e.path().strip_prefix(dir).unwrap().to_path_buf(), bytes)
        })
        .collect()
}

/// Runs `args` in `<base>/work` with `--dry-run` and checks that nothing under `base` changed,
/// then runs them for real and checks that every file that changed is in the dry run's list,
/// and every path in the list is there. Returns the list.
fn check(base: &Path, args: &[&str]) -> Vec<String> {
    let run = || {
        let mut cmd = owo(&base.join("ids"));
        cmd.current_dir(base.join("work")).args(args);
        cmd
    };
    let before = snapshot(base);
    let dry = json(run().arg("--dry-run"));
    assert_eq!(dry["ok"], true, "{args:?}: {dry}");
    assert!(snapshot(base) == before, "{args:?} wrote in a dry run");
    let real = json(&mut run());
    assert_eq!(real["ok"], true, "{args:?}: {real}");
    let after = snapshot(base);

    let list: Vec<String> = dry["data"]["would_write"]
        .as_array()
        .unwrap_or_else(|| panic!("{args:?}: {dry}"))
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect();
    let named: Vec<PathBuf> = list
        .iter()
        .map(|p| {
            let rel = Path::new(p.trim_end_matches('/')).strip_prefix(base);
            rel.unwrap_or_else(|_| panic!("{p} is outside {}", base.display()))
                .to_path_buf()
        })
        .collect();
    for (path, bytes) in &after {
        if bytes.is_some() && before.get(path) != Some(bytes) {
            assert!(
                named.contains(path),
                "{args:?} wrote {} and its dry run did not name it: {list:?}",
                path.display()
            );
        }
    }
    for (p, rel) in list.iter().zip(&named) {
        let folder = p.ends_with('/');
        match after.get(rel) {
            Some(bytes) => assert_eq!(bytes.is_none(), folder, "{args:?}: {p}"),
            None => panic!("{args:?} named {p} and did not write it"),
        }
    }
    list
}

#[test]
fn dry_run_names_what_each_writing_command_writes() {
    let tmp = TempDir::new().unwrap();
    // The canonical path, which the paths in the list start with.
    let base = fs::canonicalize(tmp.path()).unwrap();
    copy_tree(&examples().join("nicole"), &base.join("ids/nicole"));
    let work = base.join("work");
    fs::create_dir_all(&work).unwrap();
    fs::write(
        work.join("t.jsonl"),
        "{\"id\":\"a\",\"role\":\"user\",\"text\":\"hi\"}\n{\"id\":\"b\",\"role\":\"assistant\",\"text\":\"hello\"}\n",
    )
    .unwrap();
    let v1 = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v1-lumen");

    let list = check(&base, &["new", "Thea"]);
    assert!(list.iter().any(|p| p.ends_with("/thea/soul/SOUL.md")));
    assert!(list.iter().any(|p| p.ends_with("/thea/life/_index.md")));
    assert!(list.iter().any(|p| p.ends_with("/thea/work/projects/")));
    for args in [
        &["write", "soul/SOUL.md", "--content", "# Soul\n\nNew.\n"][..],
        &["add", "character", "Mara Voss"],
        &["add", "project", "Big Thing"],
        &["index"],
        &["stamp"],
        &["review", "done"],
        &["soul", "note", "SOUL.md", "--change", "Calmer"],
        &[
            "soul",
            "note",
            "SOUL.md",
            "--change",
            "Kinder",
            "--evidence",
            "first-firing",
        ],
        &["export", "card", "--out", "card.json"],
        &["export", "skill", "--out", "skill"],
        &["export", "soulspec", "--out", "spec"],
        &[
            "sync",
            "start",
            "--session",
            "s1",
            "--transcript",
            "t.jsonl",
            "--now",
        ],
        &["sync", "done", "--session", "s1", "--until", "2"],
        &[
            "sync",
            "push",
            "--session",
            "p1",
            "--now",
            "--turns",
            r#"[{"id":1,"role":"user","text":"hi"}]"#,
        ],
    ] {
        let list = check(&base, &[&["-i", "nicole"], args].concat());
        assert!(!list.is_empty(), "{args:?} would write nothing");
    }
    let out = base.join("ids/lumen");
    let list = check(
        &base,
        &[
            "migrate",
            "v1",
            v1.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert!(list
        .iter()
        .any(|p| p.ends_with("/lumen/life/characters/ada.md")));
    assert!(list.iter().any(|p| p.ends_with("/lumen/life/timeline.md")));
}

#[test]
fn dry_run_text_names_the_files_and_keeps_errors() {
    let tmp = TempDir::new().unwrap();
    let base = fs::canonicalize(tmp.path()).unwrap();
    copy_tree(&examples().join("nicole"), &base.join("nicole"));
    let skill = base.join("skill");
    owo(&base)
        .args(["-i", "nicole", "export", "skill", "--dry-run", "--out"])
        .arg(&skill)
        .assert()
        .success()
        .stdout(format!(
            "would write {}\n",
            skill.join("SKILL.md").display()
        ));
    owo(&base)
        .args(["-i", "nicole", "index", "--dry-run"])
        .assert()
        .success()
        .stdout("would write nothing\n");
    let v: Value = json(owo(&base).args(["new", "Nicole", "--dry-run"]));
    assert_eq!(v["ok"], false);
    owo(&base)
        .args(["new", "Nicole", "--dry-run"])
        .assert()
        .code(1);
    assert!(!skill.exists());
}
