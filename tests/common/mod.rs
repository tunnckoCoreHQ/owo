//! Helpers shared by the end-to-end tests.
#![allow(dead_code)]

use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The built `owo` binary with `--root` set and the owo environment cleared.
pub fn owo(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("owo").unwrap();
    cmd.arg("--root")
        .arg(root)
        .env_remove("OWO_ROOT")
        .env_remove("OWO_IDENTITY")
        .env_remove("OWO_PROJECT");
    cmd
}

/// Runs `cmd` with `--json` and parses stdout; panics with stderr when it is not JSON.
pub fn json(cmd: &mut Command) -> Value {
    let out = cmd.arg("--json").output().unwrap();
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)))
}

pub fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

pub fn copy_tree(from: &Path, to: &Path) {
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

/// A temporary root holding a copy of the example identity `id`.
pub fn copy_example(id: &str) -> TempDir {
    let tmp = TempDir::new().unwrap();
    copy_tree(&examples().join(id), &tmp.path().join(id));
    tmp
}
