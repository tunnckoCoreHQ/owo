use crate::identity::Identity;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct Change {
    pub path: String,
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
    pub pending: Vec<String>,
}

pub fn changes(id: &Identity, since: Option<&str>) -> anyhow::Result<Changes> {
    if crate::git::is_repo(&id.dir) {
        let mut args = vec![
            "log".to_string(),
            "--date=short".into(),
            "--format=%x01%ad%x02%h%x02%s".into(),
            "--name-status".into(),
            "--relative".into(),
        ];
        if let Some(s) = since {
            // Git reads a bare day as that day at the current time; start at midnight instead.
            let midnight = if crate::state::parse_day(s).is_ok() {
                " 00:00:00"
            } else {
                ""
            };
            args.push(format!("--since={s}{midnight}"));
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
        return Ok(Changes {
            source: "git",
            days,
            pending: crate::git::dirty(&id.dir).unwrap_or_default(),
        });
    }
    let since = since
        .map(|s| crate::state::parse_day(s).map_err(|e| crate::Fault::SinceNotDay(e.to_string())))
        .transpose()?
        .map(|d| d.to_string());
    let mut stamped: Vec<(String, String)> = crate::state::tracked(id)
        .filter_map(|e| {
            let at = crate::state::updated(id, &e.rel)
                .map(str::to_string)
                .or_else(|| e.field_str("updated"))?;
            Some((at, e.rel.clone()))
        })
        .collect();
    stamped.sort_by(|a, b| (&b.0, &a.1).cmp(&(&a.0, &b.1)));
    let mut days: BTreeMap<String, Vec<Change>> = BTreeMap::new();
    for (at, path) in stamped {
        let day = chrono::DateTime::parse_from_rfc3339(&at)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_else(|_| crate::state::day(&at).to_string());
        if since.as_ref().is_some_and(|s| &day < s) {
            continue;
        }
        days.entry(day).or_default().push(Change {
            path,
            status: "stamp".into(),
            commit: None,
            subject: None,
        });
    }
    Ok(Changes {
        source: "state",
        days,
        pending: vec![],
    })
}

pub fn changed_paths(id: &Identity) -> Vec<String> {
    let mut out = crate::git::dirty(&id.dir).unwrap_or_else(|| {
        let today = crate::state::today();
        crate::state::stamps(id)
            .filter(|(_, _, at)| at.starts_with(&today))
            .map(|(scope, rel, _)| format!("{scope}/{rel}"))
            .collect()
    });
    if let Ok(r) = crate::stamp::run(id, true) {
        out.extend(r.added.into_iter().chain(r.changed));
    }
    out.sort();
    out.dedup();
    out
}
