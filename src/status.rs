//! Derived current state: `owo state`, `owo recent`, and the "Now" block of `owo context`.
//! Everything here is computed from the state files and frontmatter dates.

use crate::identity::{Entry, Identity, Kind};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct Link {
    pub title: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

impl Link {
    fn of(e: &Entry) -> Self {
        Self {
            title: e.title.clone().unwrap_or_else(|| e.stem_key()),
            path: e.rel.clone(),
            date: e.field_str("date"),
        }
    }

    pub fn wiki(&self) -> String {
        match &self.date {
            Some(d) if !self.title.starts_with(d.as_str()) => format!("[[{}]] ({d})", self.title),
            _ => format!("[[{}]]", self.title),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Review {
    pub last_reviewed: Option<String>,
    pub days_since: Option<i64>,
    pub every_days: i64,
    pub due: bool,
    pub unreviewed_log_days: usize,
    pub memory_changed_outside_review: bool,
}

#[derive(Debug, Serialize)]
pub struct State {
    pub identity: String,
    pub last_activity: Option<String>,
    pub last_activity_by_scope: Vec<(String, String)>,
    pub last_project: Option<(String, String)>,
    pub recent_logs: Vec<Link>,
    pub recent_events: Vec<Link>,
    pub open_threads: Vec<Link>,
    pub open_conflicts: Vec<Link>,
    pub review: Review,
    pub last_soul_change: Option<Value>,
    /// With `--project`: sessions and recent notes for that project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectState>,
}

#[derive(Debug, Serialize)]
pub struct ProjectState {
    pub slug: String,
    pub first_touched: Option<String>,
    pub last_touched: Option<String>,
    pub sessions: u64,
    pub notes: Vec<Link>,
}

pub fn project(id: &Identity, slug: &str) -> anyhow::Result<ProjectState> {
    if id
        .entry(&format!("work/projects/{slug}/project.md"))
        .is_none()
    {
        anyhow::bail!("project not found: work/projects/{slug}/project.md");
    }
    let rec = id
        .read_json("work/state.json")
        .and_then(|d| d.get("projects")?.get(slug).cloned())
        .unwrap_or(Value::Null);
    let prefix = format!("work/projects/{slug}/notes/");
    let mut notes: Vec<&Entry> = id
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Note && e.rel.starts_with(&prefix))
        .collect();
    notes.sort_by(|a, b| b.rel.cmp(&a.rel));
    Ok(ProjectState {
        slug: slug.into(),
        first_touched: rec["first_touched"].as_str().map(str::to_string),
        last_touched: rec["last_touched"].as_str().map(str::to_string),
        sessions: rec["sessions"].as_u64().unwrap_or(0),
        notes: notes.into_iter().take(5).map(Link::of).collect(),
    })
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub slug: String,
    pub name: String,
    pub last_activity: Option<String>,
    pub projects: Vec<String>,
}

/// One row per identity in the root for `owo list`.
pub fn list(root: &std::path::Path) -> Vec<Row> {
    crate::config::list_identities(root)
        .into_iter()
        .map(|slug| match Identity::load(root, &slug) {
            Ok(id) => Row {
                name: id
                    .manifest
                    .as_ref()
                    .and_then(|m| m["name"].as_str())
                    .unwrap_or(&slug)
                    .to_string(),
                last_activity: scope_activity(&id).into_iter().map(|(_, t)| t).max(),
                projects: id
                    .entries
                    .iter()
                    .filter(|e| e.kind == Kind::Project)
                    .filter_map(|e| e.rel.split('/').nth(2).map(str::to_string))
                    .collect(),
                slug,
            },
            Err(_) => Row {
                name: slug.clone(),
                slug,
                last_activity: None,
                projects: vec![],
            },
        })
        .collect()
}

fn by_date_desc(id: &Identity, kind: Kind) -> Vec<&Entry> {
    let mut v: Vec<&Entry> = id.entries.iter().filter(|e| e.kind == kind).collect();
    v.sort_by(|a, b| (b.field_str("date"), &b.rel).cmp(&(a.field_str("date"), &a.rel)));
    v
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

/// Latest `updated` stamp per scope from the state files.
fn scope_activity(id: &Identity) -> Vec<(String, String)> {
    crate::state::SCOPES
        .iter()
        .filter_map(|s| {
            let doc = id.read_json(&format!("{s}/state.json"))?;
            let latest = doc
                .get("files")?
                .as_object()?
                .values()
                .filter_map(|r| r.get("updated")?.as_str().map(str::to_string))
                .max()?;
            Some((s.to_string(), latest))
        })
        .collect()
}

pub fn last_project(id: &Identity) -> Option<(String, String)> {
    let doc = id.read_json("work/state.json")?;
    doc.get("projects")?
        .as_object()?
        .iter()
        .filter_map(|(p, v)| Some((v.get("last_touched")?.as_str()?.to_string(), p.clone())))
        .max()
        .map(|(t, p)| (p, t))
}

pub fn compute(id: &Identity, days: i64, logs: usize) -> State {
    let today = Utc::now().date_naive();
    // Recent events count back from the newest story date, since story time can run behind or
    // ahead of real time.
    let newest = id
        .entries
        .iter()
        .filter(|e| matches!(e.kind, Kind::Log | Kind::Event))
        .filter_map(|e| parse_date(&e.field_str("date")?))
        .max()
        .unwrap_or(today);
    let since = newest - Duration::days(days);
    let soul = id.read_json("soul/state.json");
    let every = id
        .manifest
        .as_ref()
        .and_then(|m| m.get("review_every_days"))
        .and_then(Value::as_i64)
        .unwrap_or(7);
    let last_reviewed = soul
        .as_ref()
        .and_then(|d| d.pointer("/review/last_reviewed"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let reviewed_through = soul
        .as_ref()
        .and_then(|d| d.pointer("/review/reviewed_through"))
        .and_then(Value::as_str)
        .and_then(parse_date);
    let days_since = last_reviewed
        .as_deref()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_days());
    let log_entries = by_date_desc(id, Kind::Log);
    let unreviewed = log_entries
        .iter()
        .filter(|e| {
            match (
                e.field_str("date").as_deref().and_then(parse_date),
                reviewed_through,
            ) {
                (Some(d), Some(r)) => d > r,
                (Some(_), None) => true,
                _ => false,
            }
        })
        .count();
    let memory_hash = id
        .entry("soul/MEMORY.md")
        .map(|e| crate::state::hash(&e.text));
    let recorded = soul
        .as_ref()
        .and_then(|d| d.pointer("/review/memory_hash"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let by_scope = scope_activity(id);
    let open = |kind: Kind, state: &str| -> Vec<Link> {
        id.entries
            .iter()
            .filter(|e| e.kind == kind && e.field_str("state").as_deref() == Some(state))
            .map(Link::of)
            .collect()
    };
    State {
        identity: id.slug.clone(),
        last_activity: by_scope.iter().map(|(_, t)| t.clone()).max(),
        last_activity_by_scope: by_scope,
        last_project: last_project(id),
        recent_logs: log_entries.iter().take(logs).map(|e| Link::of(e)).collect(),
        recent_events: by_date_desc(id, Kind::Event)
            .into_iter()
            .filter(|e| {
                e.field_str("date")
                    .as_deref()
                    .and_then(parse_date)
                    .is_some_and(|d| d >= since)
            })
            .map(Link::of)
            .collect(),
        open_threads: open(Kind::Thread, "open"),
        open_conflicts: open(Kind::Conflict, "open"),
        review: Review {
            last_reviewed,
            days_since,
            every_days: every,
            due: days_since.is_none_or(|d| d >= every),
            unreviewed_log_days: unreviewed,
            memory_changed_outside_review: memory_hash.is_some() && memory_hash != recorded,
        },
        last_soul_change: soul
            .as_ref()
            .and_then(|d| d.get("history"))
            .and_then(Value::as_array)
            .and_then(|h| h.last().cloned()),
        project: None,
    }
}

fn join(links: &[Link]) -> String {
    if links.is_empty() {
        "none".into()
    } else {
        links.iter().map(Link::wiki).collect::<Vec<_>>().join(", ")
    }
}

/// The short generated "Now" block injected by `owo context`.
pub fn now_block(s: &State, project: Option<&str>) -> String {
    let mut out = String::from("## Now\n\n");
    let scopes: Vec<String> = s
        .last_activity_by_scope
        .iter()
        .map(|(scope, t)| format!("{scope} {}", &t[..10.min(t.len())]))
        .collect();
    out.push_str(&format!(
        "- Last activity: {}\n",
        if scopes.is_empty() {
            "none".into()
        } else {
            scopes.join(", ")
        }
    ));
    if let Some((p, t)) = &s.last_project {
        out.push_str(&format!(
            "- Last project: {p} ({})\n",
            &t[..10.min(t.len())]
        ));
    }
    out.push_str(&format!("- Recent days: {}\n", join(&s.recent_logs)));
    out.push_str(&format!("- Recent events: {}\n", join(&s.recent_events)));
    out.push_str(&format!("- Open threads: {}\n", join(&s.open_threads)));
    if !s.open_conflicts.is_empty() {
        out.push_str(&format!("- Open conflicts: {}\n", join(&s.open_conflicts)));
    }
    let r = &s.review;
    let review = match r.days_since {
        None => "never reviewed".to_string(),
        Some(d) => format!(
            "{} ({d} days since, every {})",
            if r.due { "due" } else { "not due" },
            r.every_days
        ),
    };
    out.push_str(&format!(
        "- Review: {review}, {} unreviewed log days\n",
        r.unreviewed_log_days
    ));
    let mut more = vec!["life/_index.md", "life/timeline.md", "work/_index.md"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    if let Some(p) = project {
        more.push(format!("work/projects/{p}/notes/"));
    }
    out.push_str(&format!(
        "- More: {} (or `owo search`, `owo resolve`)\n",
        more.join(", ")
    ));
    out
}

pub fn render(s: &State) -> String {
    let mut out = format!("# {}\n\n", s.identity);
    out.push_str(&now_block(s, None));
    if let Some(change) = &s.last_soul_change {
        out.push_str(&format!(
            "- Last soul change: {} {}\n",
            change.get("file").and_then(Value::as_str).unwrap_or("?"),
            change.get("change").and_then(Value::as_str).unwrap_or("")
        ));
    }
    if let Some(p) = &s.project {
        let last = p
            .last_touched
            .as_deref()
            .map(|t| &t[..10.min(t.len())])
            .unwrap_or("never");
        out.push_str(&format!(
            "- Project {}: {} sessions, last {last}, notes {}\n",
            p.slug,
            p.sessions,
            join(&p.notes)
        ));
    }
    if s.review.memory_changed_outside_review {
        out.push_str("- MEMORY.md changed outside a review\n");
    }
    out
}

#[derive(Debug, Serialize)]
pub struct Recent {
    pub path: String,
    pub updated: String,
}

/// Files whose state stamp (or frontmatter `updated`) falls in the last `days` days.
pub fn recent(id: &Identity, days: i64) -> Vec<Recent> {
    let since = (Utc::now() - Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();
    let mut out = vec![];
    for e in crate::state::tracked(id) {
        let stamped = crate::state::scope_of(&e.rel).and_then(|(scope, rel)| {
            id.read_json(&format!("{scope}/state.json"))?
                .get("files")?
                .get(rel)?
                .get("updated")?
                .as_str()
                .map(str::to_string)
        });
        let updated = stamped.or_else(|| e.field_str("updated"));
        if let Some(u) = updated.filter(|u| u.as_str() >= since.as_str()) {
            out.push(Recent {
                path: e.rel.clone(),
                updated: u,
            });
        }
    }
    out.sort_by(|a, b| (&b.updated, &a.path).cmp(&(&a.updated, &b.path)));
    out
}
