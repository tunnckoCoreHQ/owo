use crate::identity::{Entry, Identity, Kind};
use crate::state::date_of;
use chrono::{Duration, Utc};
use serde::Serialize;

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
            title: e.label(),
            path: e.rel.clone(),
            date: e.field_str("date"),
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
    pub last_soul_change: Option<crate::review::SoulChange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectState>,
}

#[derive(Debug, Serialize)]
pub struct ProjectState {
    pub slug: String,
    pub last_updated: Option<String>,
    pub notes: Vec<Link>,
}

pub fn project(id: &Identity, slug: &str) -> ProjectState {
    ProjectState {
        slug: slug.into(),
        last_updated: project_stamps(id).remove(slug),
        notes: id.notes(slug).into_iter().take(5).map(Link::of).collect(),
    }
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub slug: String,
    pub name: String,
    pub last_activity: Option<String>,
    pub projects: Vec<String>,
}

pub fn list(root: &std::path::Path) -> Vec<Row> {
    crate::config::list_identities(root)
        .into_iter()
        .map(|slug| {
            let id = Identity::load(root, &slug).ok();
            Row {
                name: id.as_ref().map_or(slug.clone(), |i| i.name().to_string()),
                last_activity: id
                    .as_ref()
                    .and_then(|i| scope_activity(i).into_iter().map(|(_, t)| t).max()),
                projects: id.map_or(vec![], |i| i.projects()),
                slug,
            }
        })
        .collect()
}

fn by_date_desc(id: &Identity, kind: Kind) -> Vec<&Entry> {
    let mut v: Vec<&Entry> = id.entries.iter().filter(|e| e.kind == kind).collect();
    v.sort_by(|a, b| (b.field_str("date"), &b.rel).cmp(&(a.field_str("date"), &a.rel)));
    v
}

fn scope_activity(id: &Identity) -> Vec<(String, String)> {
    crate::state::SCOPES
        .iter()
        .filter_map(|s| {
            let latest = crate::state::stamps(id)
                .filter(|(scope, _, _)| scope == s)
                .map(|(_, _, at)| at)
                .max()?;
            Some((s.to_string(), latest.to_string()))
        })
        .collect()
}

fn project_stamps(id: &Identity) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::<String, String>::new();
    for (scope, rel, at) in crate::state::stamps(id) {
        if scope != "work" {
            continue;
        }
        let Some((p, _)) = rel
            .strip_prefix("projects/")
            .and_then(|rest| rest.split_once('/'))
        else {
            continue;
        };
        let newest = out.entry(p.to_string()).or_default();
        if at > newest.as_str() {
            *newest = at.to_string();
        }
    }
    out
}

pub fn last_project(id: &Identity) -> Option<(String, String)> {
    project_stamps(id).into_iter().max_by(|a, b| a.1.cmp(&b.1))
}

pub(crate) fn open_links(id: &Identity, kind: Kind) -> Vec<Link> {
    id.entries
        .iter()
        .filter(|e| e.kind == kind && e.field_str("state").as_deref() == Some("open"))
        .map(Link::of)
        .collect()
}

pub fn compute(id: &Identity, days: i64, logs: usize) -> State {
    let today = Utc::now().date_naive();
    let newest = id
        .entries
        .iter()
        .filter(|e| matches!(e.kind, Kind::Log | Kind::Event))
        .filter(|e| !(e.kind == Kind::Log && e.is_blank()))
        .filter_map(|e| date_of(&e.field_str("date")?))
        .max()
        .unwrap_or(today);
    let since = newest - Duration::days(days);
    let review = crate::state::review(id);
    let log_entries: Vec<&Entry> = by_date_desc(id, Kind::Log)
        .into_iter()
        .filter(|e| !e.is_blank())
        .collect();
    let unreviewed = log_entries
        .iter()
        .filter(|e| review.unreviewed(e.field_str("date").as_deref()))
        .count();
    let by_scope = scope_activity(id);
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
        open_threads: open_links(id, Kind::Thread),
        open_conflicts: open_links(id, Kind::Conflict),
        last_soul_change: crate::review::soul_history(id, None).pop(),
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

pub fn join_links(links: &[Link]) -> String {
    let wiki = |l: &Link| match &l.date {
        Some(d) if !l.title.starts_with(d.as_str()) => format!("[[{}]] ({d})", l.title),
        _ => format!("[[{}]]", l.title),
    };
    match links {
        [] => "none".into(),
        _ => links.iter().map(wiki).collect::<Vec<_>>().join(", "),
    }
}

pub fn now_block(s: &State, project: Option<&str>) -> String {
    let scopes: Vec<String> = s
        .last_activity_by_scope
        .iter()
        .map(|(scope, t)| format!("{scope} {}", crate::state::day(t)))
        .collect();
    let mut lines = vec![format!(
        "- Last activity: {}",
        if scopes.is_empty() {
            "none".into()
        } else {
            scopes.join(", ")
        }
    )];
    if let Some((p, t)) = &s.last_project {
        lines.push(format!("- Last project: {p} ({})", crate::state::day(t)));
    }
    lines.push(format!("- Recent days: {}", join_links(&s.recent_logs)));
    lines.push(format!("- Recent events: {}", join_links(&s.recent_events)));
    lines.push(format!("- Open threads: {}", join_links(&s.open_threads)));
    if !s.open_conflicts.is_empty() {
        lines.push(format!(
            "- Open conflicts: {}",
            join_links(&s.open_conflicts)
        ));
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
    lines.push(format!(
        "- Review: {review}, {} unreviewed log days",
        r.unreviewed_log_days
    ));
    let mut more = vec!["life/_index.md", "life/timeline.md", "work/_index.md"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
    if let Some(p) = project {
        more.push(format!("work/projects/{p}/notes/"));
    }
    lines.push(format!(
        "- More: {} (or `owo search`, `owo resolve`)",
        more.join(", ")
    ));
    format!("## Now\n\n{}\n", lines.join("\n"))
}
