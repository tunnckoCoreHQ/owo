//! Derived current state: `owo state`, `owo recent`, and the "Now" block of `owo context`.
//! Everything here is computed from the state files and frontmatter dates.

use crate::identity::{Entry, Identity, Kind};
use crate::state::date_of;
use chrono::{Duration, Utc};
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
    pub(crate) fn of(e: &Entry) -> Self {
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

pub fn project(id: &Identity, slug: &str) -> ProjectState {
    let rec = &crate::state::doc(id, "work")["projects"][slug];
    let prefix = format!("work/projects/{slug}/notes/");
    let mut notes: Vec<&Entry> = id
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Note && e.rel.starts_with(&prefix))
        .collect();
    notes.sort_by(|a, b| b.rel.cmp(&a.rel));
    ProjectState {
        slug: slug.into(),
        first_touched: rec["first_touched"].as_str().map(str::to_string),
        last_touched: rec["last_touched"].as_str().map(str::to_string),
        sessions: rec["sessions"].as_u64().unwrap_or(0),
        notes: notes.into_iter().take(5).map(Link::of).collect(),
    }
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

pub fn render_list(rows: &[Row]) -> String {
    rows.iter()
        .map(|row| {
            let last = row
                .last_activity
                .as_deref()
                .map(crate::state::day)
                .unwrap_or("-");
            format!(
                "{:<16} {:<20} {:<10} {}\n",
                row.slug,
                row.name,
                last,
                row.projects.join(", ")
            )
        })
        .collect()
}

fn by_date_desc(id: &Identity, kind: Kind) -> Vec<&Entry> {
    let mut v: Vec<&Entry> = id.entries.iter().filter(|e| e.kind == kind).collect();
    v.sort_by(|a, b| (b.field_str("date"), &b.rel).cmp(&(a.field_str("date"), &a.rel)));
    v
}

/// Latest `updated` stamp per scope from the state files.
fn scope_activity(id: &Identity) -> Vec<(String, String)> {
    crate::state::SCOPES
        .iter()
        .filter_map(|s| {
            let latest = crate::state::doc(id, s)
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
    crate::state::doc(id, "work")
        .get("projects")?
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
        .filter_map(|e| date_of(&e.field_str("date")?))
        .max()
        .unwrap_or(today);
    let since = newest - Duration::days(days);
    let review = crate::state::review(id);
    // A log with nothing written yet is not a day that happened.
    let log_entries: Vec<&Entry> = by_date_desc(id, Kind::Log)
        .into_iter()
        .filter(|e| !e.is_blank())
        .collect();
    let unreviewed = log_entries
        .iter()
        .filter(|e| review.unreviewed(e.field_str("date").as_deref()))
        .count();
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
                    .and_then(date_of)
                    .is_some_and(|d| d >= since)
            })
            .map(Link::of)
            .collect(),
        open_threads: open(Kind::Thread, "open"),
        open_conflicts: open(Kind::Conflict, "open"),
        last_soul_change: crate::state::doc(id, "soul")["history"]
            .as_array()
            .and_then(|h| h.last().cloned()),
        review: Review {
            last_reviewed: review.last_reviewed,
            days_since: review.days_since,
            every_days: review.every_days,
            due: review.due,
            unreviewed_log_days: unreviewed,
            memory_changed_outside_review: review.memory_changed,
        },
        project: None,
    }
}

pub(crate) fn join(links: &[Link]) -> String {
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
        .map(|(scope, t)| format!("{scope} {}", crate::state::day(t)))
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
        out.push_str(&format!("- Last project: {p} ({})\n", crate::state::day(t)));
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
            .map(crate::state::day)
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
        let stamped = crate::state::record(id, &e.rel)
            .and_then(|r| r["updated"].as_str())
            .map(str::to_string);
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
