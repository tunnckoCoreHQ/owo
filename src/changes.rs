//! `owo changes`: what changed in an identity, grouped by day.
//! Reads Git history when the identity is in a repo, else the state-file stamps.

use crate::identity::Identity;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct Change {
    pub path: String,
    /// Git status letter (A, M, D, R) or "stamp" for state-file history.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Changes {
    pub source: &'static str,
    pub days: BTreeMap<String, Vec<Change>>,
    /// Uncommitted paths (Git only).
    pub pending: Vec<String>,
}

pub fn changes(id: &Identity, since: Option<&str>) -> Changes {
    if crate::git::is_repo(&id.dir) {
        let mut args = vec![
            "log".to_string(),
            "--date=short".into(),
            "--format=%x01%ad%x02%h%x02%s".into(),
            "--name-status".into(),
            "--relative".into(),
        ];
        if let Some(s) = since {
            args.push(format!("--since={s}"));
        }
        args.extend(["--".into(), ".".into()]);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut days: BTreeMap<String, Vec<Change>> = BTreeMap::new();
        if let Some(out) = crate::git::run(&id.dir, &args) {
            for block in out.split('\u{1}').filter(|b| !b.trim().is_empty()) {
                let mut lines = block.lines();
                let head: Vec<&str> = lines.next().unwrap_or_default().split('\u{2}').collect();
                let (date, commit, subject) = (
                    head[0].to_string(),
                    head.get(1).map(|s| s.to_string()),
                    head.get(2).map(|s| s.to_string()),
                );
                for l in lines.filter(|l| !l.trim().is_empty()) {
                    let mut parts = l.split('\t');
                    let status = parts
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .next()
                        .unwrap_or('M')
                        .to_string();
                    let path = parts.next_back().unwrap_or_default().to_string();
                    days.entry(date.clone()).or_default().push(Change {
                        path,
                        status,
                        commit: commit.clone(),
                        subject: subject.clone(),
                    });
                }
            }
        }
        return Changes {
            source: "git",
            days,
            pending: crate::git::dirty(&id.dir).unwrap_or_default(),
        };
    }
    let mut days: BTreeMap<String, Vec<Change>> = BTreeMap::new();
    for r in crate::status::recent(id, 36500) {
        let day = r.updated.chars().take(10).collect::<String>();
        if since.is_some_and(|s| day.as_str() < s) {
            continue;
        }
        days.entry(day).or_default().push(Change {
            path: r.path,
            status: "stamp".into(),
            commit: None,
            subject: None,
        });
    }
    Changes {
        source: "state",
        days,
        pending: vec![],
    }
}

pub fn render(c: &Changes) -> String {
    let mut o = String::new();
    for (day, list) in c.days.iter().rev() {
        o.push_str(&format!("{day}\n"));
        for ch in list {
            let commit = ch
                .commit
                .as_ref()
                .map(|h| format!("  {h}"))
                .unwrap_or_default();
            o.push_str(&format!("  {:<5} {}{commit}\n", ch.status, ch.path));
        }
    }
    if !c.pending.is_empty() {
        o.push_str("uncommitted\n");
        for p in &c.pending {
            o.push_str(&format!("  {p}\n"));
        }
    }
    if o.is_empty() {
        o.push_str("no changes\n");
    }
    o
}

/// Files changed since the last commit or stamp: uncommitted Git paths plus stamp drift.
pub fn changed_paths(id: &Identity) -> Vec<String> {
    let mut out = crate::git::dirty(&id.dir).unwrap_or_default();
    if let Ok(r) = crate::stamp::run(id, true) {
        out.extend(r.added.into_iter().chain(r.changed));
    }
    out.sort();
    out.dedup();
    out
}
