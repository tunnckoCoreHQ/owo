use crate::identity::NOT_ENTRIES;
use std::path::Path;
use std::process::Command;

pub fn run(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        // Print non-ASCII paths as they are, not octal-quoted.
        .args(["-c", "core.quotepath=off"])
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs a Git command on the entries of the identity in `dir`: every path under it but the
/// folders that hold no entries.
pub fn run_on_entries(dir: &Path, args: &[&str]) -> Option<String> {
    let exclude = NOT_ENTRIES.map(|folder| format!(":(exclude){folder}"));
    let mut all = args.to_vec();
    all.extend(["--", "."]);
    all.extend(exclude.iter().map(String::as_str));
    run(dir, &all)
}

pub fn is_repo(dir: &Path) -> bool {
    run(dir, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|s| s.trim() == "true")
}

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

pub fn dirty(dir: &Path) -> Option<Vec<String>> {
    // `diff --relative` and `ls-files` print paths relative to `dir`; with `-z` they are
    // NUL-separated and never quoted. Before the first commit there is no HEAD, and every
    // tracked file counts as uncommitted.
    let changed = run_on_entries(dir, &["diff", "--name-only", "-z", "--relative", "HEAD"])
        .or_else(|| run_on_entries(dir, &["ls-files", "-z", "--cached"]))?;
    let untracked = run_on_entries(dir, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    let mut out: Vec<String> = changed
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    Some(out)
}
