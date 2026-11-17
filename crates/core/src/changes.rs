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

fn since_day(since: Option<&str>) -> anyhow::Result<Option<String>> {
    Ok(since
        .map(|s| crate::state::parse_day(s).map_err(|e| crate::Fault::SinceNotDay(e.to_string())))
        .transpose()?
        .map(|d| d.to_string()))
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
        // Git reads a day only from 1970 to 2099 and misreads 1970-01-01 at a local midnight
        // east of UTC. Every commit is after an earlier day and before a later one.
        let first_day = chrono::NaiveDate::from_ymd_opt(1970, 1, 2).expect("a date");
        let past_day = chrono::NaiveDate::from_ymd_opt(2100, 1, 1).expect("a date");
        let mut no_commit = false;
        match since.map(|s| (s, crate::state::parse_day(s))) {
            Some((_, Ok(day))) if day < first_day => {}
            Some((_, Ok(day))) if day >= past_day => no_commit = true,
            // Git reads a bare day as that day at the current time; start at midnight instead.
            Some((s, Ok(_))) => args.push(format!("--since={s} 00:00:00")),
            Some((s, Err(_))) => args.push(format!("--since={s}")),
            None => {}
        }
        args.extend(["--".into(), ".".into()]);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut days: BTreeMap<String, Vec<Change>> = BTreeMap::new();
        let log = (!no_commit)
            .then(|| crate::git::run(&id.dir, &args))
            .flatten();
        if let Some(out) = log {
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
                    let status = parts.next().and_then(|s| s.chars().next());
                    let path = parts.next_back().unwrap_or_default().to_string();
                    days.entry(date.clone()).or_default().push(Change {
                        path,
                        status: status.unwrap_or('M').to_string(),
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
    // Outside Git only a day compares; `git log --since` words such as "yesterday" do not.
    let since = since_day(since)?;
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
