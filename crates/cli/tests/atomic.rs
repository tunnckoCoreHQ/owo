#![cfg(unix)]

mod common;

use common::{copy_tree, examples, owo, PNG_1X1};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn setup() -> TempDir {
    let tmp = TempDir::new().unwrap();
    copy_tree(&examples().join("nicole"), &tmp.path().join("ids/nicole"));
    fs::create_dir_all(tmp.path().join("work")).unwrap();
    tmp
}

fn nicole(tmp: &Path) -> PathBuf {
    tmp.join("ids/nicole")
}

fn run(tmp: &Path, args: &[&str]) -> bool {
    owo(&tmp.join("ids"))
        .current_dir(tmp.join("work"))
        .args(args)
        .output()
        .unwrap()
        .status
        .success()
}

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

fn no_tmp(dir: &Path) {
    for e in walkdir::WalkDir::new(dir) {
        let e = e.unwrap();
        assert!(
            !e.file_name().to_string_lossy().ends_with(".tmp"),
            "{} stays",
            e.path().display()
        );
    }
}

fn ino(path: &Path) -> u64 {
    fs::metadata(path).unwrap().ino()
}

fn read_only(dir: &Path, f: impl FnOnce()) {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
    f();
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
}

fn block(path: &Path) {
    fs::create_dir_all(path.join("x")).unwrap();
}

#[test]
fn atomic_card_failed_rename_leaves_no_tmp() {
    let tmp = setup();
    block(&tmp.path().join("work/x.json"));
    assert!(!run(
        tmp.path(),
        &["-i", "nicole", "export", "persona-card", "--out", "x.json"]
    ));
    no_tmp(tmp.path());
}

#[test]
fn atomic_write_replaces_the_file() {
    let tmp = setup();
    let file = nicole(tmp.path()).join("life/insights/let-it-dry.md");
    let before = ino(&file);
    assert!(run(
        tmp.path(),
        &[
            "-i",
            "nicole",
            "write",
            "life/insights/let-it-dry.md",
            "--content",
            "# Let It Dry\n"
        ]
    ));
    assert_eq!(fs::read_to_string(&file).unwrap(), "# Let It Dry\n");
    assert_ne!(ino(&file), before);
    no_tmp(tmp.path());
}

#[test]
fn atomic_index_replaces_the_files() {
    let tmp = setup();
    let dir = nicole(tmp.path());
    let files = ["life/insights/_index.md", "life/timeline.md"].map(|r| dir.join(r));
    let before = files.clone().map(|f| {
        fs::write(&f, "stale\n").unwrap();
        ino(&f)
    });
    assert!(run(tmp.path(), &["-i", "nicole", "index"]));
    for (f, b) in files.iter().zip(before) {
        assert_ne!(ino(f), b, "{} written in place", f.display());
    }
    no_tmp(tmp.path());
}

#[test]
fn atomic_stamp_replaces_the_entry() {
    let tmp = setup();
    let file = nicole(tmp.path()).join("life/insights/let-it-dry.md");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("{text}\nMore.\n")).unwrap();
    let before = ino(&file);
    assert!(run(tmp.path(), &["-i", "nicole", "stamp"]));
    assert_ne!(
        fs::read_to_string(&file).unwrap(),
        format!("{text}\nMore.\n")
    );
    assert_ne!(ino(&file), before);
    no_tmp(tmp.path());
}

#[test]
fn atomic_export_skill_replaces_the_file() {
    let tmp = setup();
    let args = ["-i", "nicole", "export", "skill", "--out", "sk"];
    assert!(run(tmp.path(), &args));
    let file = tmp.path().join("work/sk/SKILL.md");
    let before = ino(&file);
    assert!(run(tmp.path(), &args));
    assert_ne!(ino(&file), before);
    no_tmp(tmp.path());
}

#[test]
fn atomic_export_soulspec_replaces_the_files() {
    let tmp = setup();
    let args = ["-i", "nicole", "export", "soulspec", "--out", "pkg"];
    assert!(run(tmp.path(), &args));
    let file = tmp.path().join("work/pkg/SOUL.md");
    let before = ino(&file);
    assert!(run(tmp.path(), &args));
    assert_ne!(ino(&file), before);
    no_tmp(tmp.path());
}

#[test]
fn atomic_png_card_replaces_the_file() {
    let tmp = setup();
    fs::write(tmp.path().join("work/a.png"), PNG_1X1).unwrap();
    let args = [
        "-i",
        "nicole",
        "export",
        "persona-card",
        "--png",
        "a.png",
        "--out",
        "c.png",
    ];
    assert!(run(tmp.path(), &args));
    let file = tmp.path().join("work/c.png");
    let before = ino(&file);
    assert!(run(tmp.path(), &args));
    assert_ne!(ino(&file), before);
    no_tmp(tmp.path());
}

fn transcript(tmp: &Path, turns: usize) {
    let lines: String = (0..turns)
        .map(|i| format!("{{\"role\":\"user\",\"text\":\"hi {i}\"}}\n"))
        .collect();
    fs::write(tmp.join("work/t.jsonl"), lines).unwrap();
}

const START: [&str; 8] = [
    "-i",
    "nicole",
    "sync",
    "start",
    "--session",
    "s1",
    "--transcript",
    "t.jsonl",
];

#[test]
fn atomic_sync_start_replaces_the_prompt() {
    let tmp = setup();
    transcript(tmp.path(), 1);
    assert!(run(tmp.path(), &[&START[..], &["--now"]].concat()));
    let prompt = tmp.path().join("ids/.owo/sessions/s1.prompt.md");
    let before = ino(&prompt);
    transcript(tmp.path(), 2);
    assert!(run(tmp.path(), &[&START[..], &["--now"]].concat()));
    assert_ne!(ino(&prompt), before);
    no_tmp(tmp.path());
}

#[test]
fn atomic_sync_start_is_all_or_nothing() {
    let tmp = setup();
    let ids = tmp.path().join("ids");
    transcript(tmp.path(), 1);
    block(&ids.join(".owo/sessions/s1.prompt.md"));
    let before = snapshot(&ids);
    assert!(!run(tmp.path(), &[&START[..], &["--now"]].concat()));
    assert_eq!(snapshot(&ids), before);
}

#[test]
fn atomic_sync_push_is_all_or_nothing() {
    let tmp = setup();
    let ids = tmp.path().join("ids");
    block(&ids.join(".owo/sessions/s1.prompt.md"));
    let before = snapshot(&ids);
    assert!(!run(
        tmp.path(),
        &[
            "-i",
            "nicole",
            "sync",
            "push",
            "--session",
            "s1",
            "--now",
            "--turns",
            r#"[{"id":"a","role":"user","text":"hi"}]"#,
        ]
    ));
    assert_eq!(snapshot(&ids), before);
}

#[test]
fn atomic_failed_sync_leaves_no_session_folder() {
    let tmp = setup();
    let ids = tmp.path().join("ids");
    let before = snapshot(&ids);
    assert!(!run(tmp.path(), &START));
    assert_eq!(snapshot(&ids), before);
}

#[test]
fn atomic_stamp_is_all_or_nothing() {
    let tmp = setup();
    let dir = nicole(tmp.path());
    let file = dir.join("life/insights/let-it-dry.md");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, format!("{text}\nMore.\n")).unwrap();
    let before = snapshot(&dir);
    read_only(&dir.join("persona"), || {
        assert!(!run(tmp.path(), &["-i", "nicole", "stamp"]));
    });
    assert_eq!(snapshot(&dir), before);
}

#[test]
fn atomic_index_is_all_or_nothing() {
    let tmp = setup();
    let dir = nicole(tmp.path());
    for rel in ["life/insights/_index.md", "work/_index.md"] {
        fs::write(dir.join(rel), "stale\n").unwrap();
    }
    let last = dir.join("work/_index.md");
    fs::set_permissions(&last, fs::Permissions::from_mode(0o444)).unwrap();
    let before = snapshot(&dir);
    read_only(&dir.join("work"), || {
        assert!(!run(tmp.path(), &["-i", "nicole", "index"]));
    });
    assert_eq!(snapshot(&dir), before);
}

#[test]
fn atomic_export_soulspec_is_all_or_nothing() {
    let tmp = setup();
    let work = tmp.path().join("work");
    block(&work.join("pkg/SOUL.md"));
    let before = snapshot(&work);
    assert!(!run(
        tmp.path(),
        &["-i", "nicole", "export", "soulspec", "--out", "pkg"]
    ));
    assert_eq!(snapshot(&work), before);
}

// macOS file systems refuse a name that is not UTF-8, so the bytes cannot reach owo there.
#[cfg(not(target_os = "macos"))]
#[test]
fn atomic_core_turns_a_non_utf8_path_into_utf8_text() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let tmp = setup();
    let work = tmp.path().join("work");
    let bad = |s: &[u8]| OsStr::from_bytes(s).to_os_string();
    fs::create_dir_all(work.join("v1\u{FFFD}/references")).unwrap();
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
    fs::write(
        work.join("t\u{FFFD}.jsonl"),
        "{\"role\":\"user\",\"text\":\"hi\"}\n",
    )
    .unwrap();
    let out = owo(&tmp.path().join("ids"))
        .current_dir(&work)
        .args(["-i", "nicole", "sync", "start", "--session", "s1"])
        .arg("--transcript")
        .arg(bad(b"t\xff.jsonl"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = owo(&tmp.path().join("ids"))
        .current_dir(&work)
        .args(["-i", "nicole", "export", "skill", "--out"])
        .arg(bad(b"sk\xff"))
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(work.join("sk\u{FFFD}/SKILL.md").is_file());
    assert!(!work.join(bad(b"sk\xff")).exists());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn atomic_core_api_turns_a_non_utf8_path_into_utf8_text() {
    use owo_core::{Env, Root, Select, SkillRequest};
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let tmp = setup();
    let work = tmp.path().join("work");
    let sel = Select {
        root: Some(tmp.path().join("ids")),
        identity: Some("nicole".into()),
        project: None,
    };
    let env = Env {
        cwd: work.clone(),
        ..Env::default()
    };
    let id = Root::open(&sel, &env).unwrap().identity().unwrap();
    let out = PathBuf::from(OsStr::from_bytes(b"api\xff"));
    let shown = id.export_skill(&SkillRequest { out }).unwrap();
    assert_eq!(shown, "api\u{FFFD}/SKILL.md");
    assert!(work.join("api\u{FFFD}/SKILL.md").is_file());
}
