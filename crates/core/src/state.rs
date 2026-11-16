use crate::identity::{Identity, Kind};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde_json::{json, Map, Value};

pub const SCOPES: [&str; 3] = ["persona", "life", "work"];

pub fn hash(text: &str) -> String {
    format!("b3:{}", blake3::hash(text.as_bytes()).to_hex())
}

pub fn day(stamp: &str) -> &str {
    stamp.get(..10).unwrap_or(stamp)
}

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn local_today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// The local day `days` ago, as `YYYY-MM-DD`, which Git reads a plain `--since` day in.
pub fn local_day_ago(days: i64) -> String {
    (chrono::Local::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

pub fn today() -> String {
    day(&now()).to_string()
}

pub fn parse_day(value: &str) -> Result<NaiveDate, String> {
    let shaped = value.len() == 10
        && value.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        });
    match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        Ok(day) if shaped => Ok(day),
        _ => Err(format!("{value:?} is not a YYYY-MM-DD date")),
    }
}

pub fn scope_of(rel: &str) -> Option<(&'static str, &str)> {
    SCOPES.iter().find_map(|s| {
        rel.strip_prefix(s)
            .and_then(|r| r.strip_prefix('/'))
            .map(|r| (*s, r))
    })
}

pub fn tracked(identity: &Identity) -> impl Iterator<Item = &crate::identity::Entry> {
    identity
        .entries
        .iter()
        .filter(|e| e.kind != Kind::Generated && e.kind != Kind::Unknown)
}

pub fn initial(identity: &Identity, at: &str) -> Vec<(String, Value)> {
    let memory_hash = identity.entry("persona/MEMORY.md").map(|e| hash(&e.text));
    SCOPES
        .iter()
        .map(|scope| {
            let files: Map<String, Value> = tracked(identity)
                .filter_map(|e| match scope_of(&e.rel) {
                    Some((s, rel)) if s == *scope => {
                        Some((rel.to_string(), json!({ "updated": at, "hash": hash(&e.text) })))
                    }
                    _ => None,
                })
                .collect();
            let doc = match *scope {
                "persona" => json!({
                    "files": files,
                    "changelog": [],
                    "review": { "last_reviewed": null, "reviewed_through": null, "memory_hash": memory_hash }
                }),
                _ => json!({ "files": files }),
            };
            (format!("{scope}/state.json"), doc)
        })
        .collect()
}

pub fn doc<'a>(id: &'a Identity, scope: &str) -> &'a Value {
    &id.state[SCOPES
        .iter()
        .position(|s| *s == scope)
        .expect("a known scope")]
}

pub fn updated<'a>(id: &'a Identity, rel: &str) -> Option<&'a str> {
    let (scope, rest) = scope_of(rel)?;
    doc(id, scope)["files"][rest]["updated"].as_str()
}

pub fn stamps(id: &Identity) -> impl Iterator<Item = (&'static str, &str, &str)> {
    SCOPES.iter().flat_map(move |scope| {
        doc(id, scope)["files"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(move |(rel, r)| Some((*scope, rel.as_str(), r["updated"].as_str()?)))
    })
}

pub fn changelog(id: &Identity) -> &[Value] {
    doc(id, "persona")["changelog"]
        .as_array()
        .map_or(&[], Vec::as_slice)
}

pub fn date_of(text: &str) -> Option<NaiveDate> {
    parse_day(text.get(..10)?).ok()
}

pub struct Review {
    pub last_reviewed: Option<String>,
    pub reviewed_through: Option<NaiveDate>,
    pub days_since: Option<i64>,
    pub every_days: i64,
    pub due: bool,
    pub memory_changed: bool,
}

impl Review {
    pub fn unreviewed(&self, date: Option<&str>) -> bool {
        match (date.and_then(date_of), self.reviewed_through) {
            (Some(d), Some(r)) => d > r,
            (Some(_), None) => true,
            _ => false,
        }
    }
}

pub fn review(id: &Identity) -> Review {
    let field = |key: &str| doc(id, "persona")["review"][key].as_str();
    let last_reviewed = field("last_reviewed").map(str::to_string);
    let every_days = id.manifest["review_every_days"].as_i64().unwrap_or(7);
    let days_since = last_reviewed
        .as_deref()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_days());
    let memory_hash = id.entry("persona/MEMORY.md").map(|e| hash(&e.text));
    Review {
        reviewed_through: field("reviewed_through").and_then(date_of),
        days_since,
        every_days,
        due: days_since.is_none_or(|d| d >= every_days),
        memory_changed: memory_hash.is_some() && memory_hash.as_deref() != field("memory_hash"),
        last_reviewed,
    }
}
