use std::path::Path;
use std::process::Command;

pub fn run(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "core.quotepath=off"])
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
    let changed = run(
        dir,
        &["diff", "--name-only", "-z", "--relative", "HEAD", "--", "."],
    )
    .or_else(|| run(dir, &["ls-files", "-z", "--cached", "--", "."]))?;
    let untracked = run(
        dir,
        &[
            "ls-files",
            "-z",
            "--others",
            "--exclude-standard",
            "--",
            ".",
        ],
    )?;
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
