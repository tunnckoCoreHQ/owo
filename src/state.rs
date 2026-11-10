//! State files: `soul/state.json`, `life/state.json`, `work/state.json`.
//! Each tracks authored Markdown files by content hash with an `updated` stamp.

use crate::identity::{Identity, Kind};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde_json::{json, Map, Value};

pub const SCOPES: [&str; 3] = ["soul", "life", "work"];

pub fn hash(text: &str) -> String {
    format!("b3:{}", blake3::hash(text.as_bytes()).to_hex())
}

/// The `YYYY-MM-DD` part of a stamp, or the whole text when it is shorter.
pub fn day(stamp: &str) -> &str {
    stamp.get(..10).unwrap_or(stamp)
}

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// The UTC day of `now`, as `YYYY-MM-DD`.
pub fn today() -> String {
    day(&now()).to_string()
}

/// A zero-padded `YYYY-MM-DD` day. Also the clap parser for date flags.
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

/// Splits `life/events/x.md` into (`life`, `events/x.md`).
pub fn scope_of(rel: &str) -> Option<(&'static str, &str)> {
    SCOPES.iter().find_map(|s| {
        rel.strip_prefix(s)
            .and_then(|r| r.strip_prefix('/'))
            .map(|r| (*s, r))
    })
}

/// Authored files tracked in state: everything except generated indexes.
pub fn tracked(identity: &Identity) -> impl Iterator<Item = &crate::identity::Entry> {
    identity
        .entries
        .iter()
        .filter(|e| e.kind != Kind::Generated && e.kind != Kind::Unknown)
}

/// Fresh state documents for a just-created identity, all files stamped `at`.
pub fn initial(identity: &Identity, at: &str) -> Vec<(String, Value)> {
    let mut files: Map<String, Value> = SCOPES
        .iter()
        .map(|s| (s.to_string(), Value::Object(Map::new())))
        .collect();
    for e in tracked(identity) {
        if let Some((scope, rel)) = scope_of(&e.rel) {
            if let Some(Value::Object(m)) = files.get_mut(scope) {
                m.insert(
                    rel.to_string(),
                    json!({ "updated": at, "hash": hash(&e.text) }),
                );
            }
        }
    }
    let memory_hash = identity.entry("soul/MEMORY.md").map(|e| hash(&e.text));
    let take = |files: &mut Map<String, Value>, s: &str| {
        files.remove(s).unwrap_or(Value::Object(Map::new()))
    };
    let soul = json!({
        "files": take(&mut files, "soul"),
        "history": [],
        "review": { "last_reviewed": null, "reviewed_through": null, "memory_hash": memory_hash }
    });
    let life = json!({ "files": take(&mut files, "life") });
    let work = json!({ "projects": {}, "files": take(&mut files, "work") });
    vec![
        ("soul/state.json".into(), soul),
        ("life/state.json".into(), life),
        ("work/state.json".into(), work),
    ]
}

/// The loaded `<scope>/state.json`, or Null when it is missing or does not parse.
pub fn doc<'a>(id: &'a Identity, scope: &str) -> &'a Value {
    &id.state[SCOPES
        .iter()
        .position(|s| *s == scope)
        .expect("a known scope")]
}

/// The state record of an identity-relative path: `{ "updated", "hash" }`.
pub fn record<'a>(id: &'a Identity, rel: &str) -> Option<&'a Value> {
    let (scope, rest) = scope_of(rel)?;
    doc(id, scope).get("files")?.get(rest)
}

/// The day at the start of a date or stamp, when it parses.
pub fn date_of(text: &str) -> Option<NaiveDate> {
    parse_day(text.get(..10)?).ok()
}

/// Review status from `soul/state.json` → `review` and `identity.json` → `review_every_days`.
pub struct Review {
    pub last_reviewed: Option<String>,
    pub reviewed_through: Option<NaiveDate>,
    /// Days since `last_reviewed`; None when never reviewed or not an RFC 3339 timestamp.
    pub days_since: Option<i64>,
    pub every_days: i64,
    pub due: bool,
    /// MEMORY.md exists and its hash differs from the one the last review recorded.
    pub memory_changed: bool,
}

impl Review {
    /// True when a story date parses and falls after `reviewed_through` (or nothing is reviewed).
    pub fn unreviewed(&self, date: Option<&str>) -> bool {
        match (date.and_then(date_of), self.reviewed_through) {
            (Some(d), Some(r)) => d > r,
            (Some(_), None) => true,
            _ => false,
        }
    }
}

pub fn review(id: &Identity) -> Review {
    let field = |key: &str| doc(id, "soul")["review"][key].as_str();
    let last_reviewed = field("last_reviewed").map(str::to_string);
    let every_days = id.manifest["review_every_days"].as_i64().unwrap_or(7);
    let days_since = last_reviewed
        .as_deref()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_days());
    let memory_hash = id.entry("soul/MEMORY.md").map(|e| hash(&e.text));
    Review {
        reviewed_through: field("reviewed_through").and_then(date_of),
        days_since,
        every_days,
        due: days_since.is_none_or(|d| d >= every_days),
        memory_changed: memory_hash.is_some() && memory_hash.as_deref() != field("memory_hash"),
        last_reviewed,
    }
}
