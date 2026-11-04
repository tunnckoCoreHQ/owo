//! State files: `soul/state.json`, `life/state.json`, `work/state.json`.
//! Each tracks authored Markdown files by content hash with created/updated stamps.

use crate::identity::{Identity, Kind};
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Map, Value};

pub const SCOPES: &[&str] = &["soul", "life", "work"];

pub fn hash(text: &str) -> String {
    format!("b3:{}", blake3::hash(text.as_bytes()).to_hex())
}

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
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
                    json!({ "created": at, "updated": at, "hash": hash(&e.text) }),
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
    let life = json!({ "files": take(&mut files, "life"), "last_write": at });
    let work = json!({ "projects": {}, "files": take(&mut files, "work") });
    vec![
        ("soul/state.json".into(), soul),
        ("life/state.json".into(), life),
        ("work/state.json".into(), work),
    ]
}
