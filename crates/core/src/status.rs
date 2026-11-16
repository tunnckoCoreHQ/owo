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
pub struct ReflectionStatus {
    pub last_reflected: Option<String>,
    pub days_since: Option<i64>,
    pub every_days: i64,
    pub due: bool,
    pub unreflected_log_days: usize,
    pub memory_changed_outside_reflection: bool,
}

#[derive(Debug, Serialize)]
pub struct CurrentState {
    pub identity: String,
    pub last_activity: Option<String>,
    pub last_activity_by_scope: Vec<(String, String)>,
    pub last_project: Option<(String, String)>,
    pub recent_logs: Vec<Link>,
    pub recent_events: Vec<Link>,
    pub open_threads: Vec<Link>,
    pub open_conflicts: Vec<Link>,
    pub reflection: ReflectionStatus,
    pub last_persona_change: Option<crate::reflect::PersonaChange>,
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
pub struct ListedIdentity {
    pub slug: String,
    pub name: String,
    pub last_activity: Option<String>,
    pub projects: Vec<String>,
}

pub fn list(root: &std::path::Path) -> Vec<ListedIdentity> {
    crate::config::list_identities(root)
        .into_iter()
        .map(|slug| match Identity::load(root, &slug) {
            Ok(id) => ListedIdentity {
                name: id.name().to_string(),
                last_activity: scope_activity(&id).into_iter().map(|(_, t)| t).max(),
                projects: id.projects(),
                slug,
            },
            Err(_) => ListedIdentity {
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
        let project = rel
            .strip_prefix("projects/")
            .and_then(|r| r.split_once('/'));
        if let Some((p, _)) = project.filter(|_| scope == "work") {
            let newest = out.entry(p.to_string()).or_default();
            *newest = at.max(newest.as_str()).to_string();
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

pub fn compute(id: &Identity, days: i64, logs: usize) -> CurrentState {
    let today = Utc::now().date_naive();
    let story_today = id
        .entries
        .iter()
        .filter(|e| matches!(e.kind, Kind::Log | Kind::Event))
        .filter(|e| !(e.kind == Kind::Log && e.is_blank()))
        .filter_map(|e| date_of(&e.field_str("date")?))
        .max()
        .unwrap_or(today);
    let since = story_today - Duration::days(days);
    let reflection = crate::state::reflection(id);
    let log_entries: Vec<&Entry> = by_date_desc(id, Kind::Log)
        .into_iter()
        .filter(|e| !e.is_blank())
        .collect();
    let unreflected = log_entries
        .iter()
        .filter(|e| reflection.unreflected(e.field_str("date").as_deref()))
        .count();
    let by_scope = scope_activity(id);
    CurrentState {
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
        last_persona_change: crate::reflect::persona_changelog(id, None).pop(),
        reflection: ReflectionStatus {
            last_reflected: reflection.last_reflected,
            days_since: reflection.days_since,
            every_days: reflection.every_days,
            due: reflection.due,
            unreflected_log_days: unreflected,
            memory_changed_outside_reflection: reflection.memory_changed,
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

pub fn now_block(s: &CurrentState, project: Option<&str>) -> String {
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
    let r = &s.reflection;
    let reflection = match r.days_since {
        None => "never reflected".to_string(),
        Some(d) => format!(
            "{} ({d} days since, every {})",
            if r.due { "due" } else { "not due" },
            r.every_days
        ),
    };
    lines.push(format!(
        "- Reflection: {reflection}, {} log days since the last reflection",
        r.unreflected_log_days
    ));
    let notes = project
        .map(|p| format!(", work/projects/{p}/notes/"))
        .unwrap_or_default();
    lines.push(format!(
        "- More: life/_index.md, life/timeline.md, work/_index.md{notes} (or `owo search`, `owo resolve`)"
    ));
    format!("## Now\n\n{}\n", lines.join("\n"))
}
