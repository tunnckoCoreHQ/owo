//! Thin wrapper over the `git` binary for history-based commands.

use std::path::Path;
use std::process::Command;

/// Runs git in `dir` and returns stdout, or None when git fails or `dir` is outside a repo.
pub fn run(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn is_repo(dir: &Path) -> bool {
    run(dir, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|s| s.trim() == "true")
}

/// Lines added and removed in `rel` (relative to `dir`) by commits since `since`.
pub fn numstat_since(dir: &Path, since: &str, rel: &str) -> Option<(usize, usize)> {
    let out = run(
        dir,
        &[
            "log",
            &format!("--since={since}"),
            "--numstat",
            "--format=",
            "--",
            rel,
        ],
    )?;
    let mut sum = (0, 0);
    for line in out.lines() {
        let mut parts = line.split('\t');
        sum.0 += parts.next()?.parse::<usize>().unwrap_or(0);
        sum.1 += parts.next()?.parse::<usize>().unwrap_or(0);
    }
    Some(sum)
}

/// Paths (relative to `dir`) with uncommitted or untracked changes under `dir`.
pub fn dirty(dir: &Path) -> Option<Vec<String>> {
    let out = run(
        dir,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--relative",
            "--",
            ".",
        ],
    )?;
    Some(
        out.lines()
            .filter_map(|l| l.get(3..))
            .map(|p| {
                p.rsplit(" -> ")
                    .next()
                    .unwrap_or(p)
                    .trim_matches('"')
                    .to_string()
            })
            .collect(),
    )
}
