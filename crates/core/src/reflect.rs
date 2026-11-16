use crate::identity::Identity;
use crate::state::parse_day;
use crate::status::{open_links, Link};
use crate::{Kind, Named};
use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Changed {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub summary: Option<String>,
    pub words: usize,
    pub updated: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<(usize, usize)>,
}

#[derive(Debug, Serialize)]
pub struct LogDay {
    pub path: String,
    pub date: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct ReflectionDigest {
    pub identity: String,
    pub last_reflected: Option<String>,
    pub reflected_through: Option<String>,
    pub memory: String,
    pub memory_words: usize,
    pub memory_changed_outside_reflection: bool,
    pub changed: Vec<Changed>,
    pub unreflected_days: Vec<LogDay>,
    pub events: Vec<Link>,
    pub lessons: Vec<Link>,
    pub journal: Vec<String>,
    pub open_threads: Vec<Link>,
    pub open_conflicts: Vec<Link>,
    pub recurring: Vec<(String, usize)>,
    pub last_persona_change: Option<PersonaChange>,
}

const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "had", "has", "have",
    "he", "her", "his", "if", "in", "into", "is", "it", "its", "me", "my", "of", "on", "or", "our",
    "she", "so", "that", "the", "their", "them", "then", "they", "this", "to", "was", "we", "were",
    "with", "you", "your",
];

fn recurring(days: &[LogDay]) -> Vec<(String, usize)> {
    let content = |w: &str| !STOP_WORDS.contains(&w);
    let mut seen: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for d in days {
        for line in d.body.lines() {
            let toks = crate::search::tokens(line);
            for w in toks.windows(3).filter(|w| content(&w[0]) && content(&w[2])) {
                seen.entry(w.join(" ")).or_default().insert(&d.date);
            }
        }
    }
    let mut out: Vec<(String, usize)> = seen
        .into_iter()
        .map(|(k, v)| (k, v.len()))
        .filter(|(_, n)| *n >= 3)
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out.truncate(20);
    out
}

pub fn digest(id: &Identity) -> ReflectionDigest {
    let reflection = crate::state::reflection(id);
    let last = reflection.last_reflected.clone();
    let after_through = |d: &Option<String>| reflection.unreflected(d.as_deref());
    let git = crate::git::is_repo(&id.dir);
    let mut changed = vec![];
    for e in crate::state::tracked(id) {
        let updated = crate::state::updated(id, &e.rel)
            .unwrap_or_default()
            .to_string();
        let reflected =
            !updated.is_empty() && last.as_deref().is_some_and(|l| updated.as_str() <= l);
        if reflected || e.rel == "persona/MEMORY.md" {
            continue;
        }
        changed.push(Changed {
            path: e.rel.clone(),
            kind: e.kind.name(),
            summary: e.field_str("summary").filter(|s| !s.is_empty()),
            words: crate::markdown::word_count(e.body()),
            git: match (&last, git) {
                (Some(l), true) => crate::git::numstat_since(&id.dir, l, &e.rel),
                _ => None,
            },
            updated,
        });
    }
    changed.sort_by(|a, b| b.updated.cmp(&a.updated).then(a.path.cmp(&b.path)));
    let mut days: Vec<LogDay> = id
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Log && !e.is_blank() && after_through(&e.field_str("date")))
        .map(|e| LogDay {
            path: e.rel.clone(),
            date: e.field_str("date").unwrap_or_default(),
            title: e.title.clone().unwrap_or_default(),
            body: e
                .body()
                .lines()
                .filter(|l| !l.starts_with("# "))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string(),
        })
        .collect();
    days.sort_by(|a, b| a.date.cmp(&b.date));
    let of_kind = |k: Kind| id.entries.iter().filter(move |e| e.kind == k);
    let changed_paths: BTreeSet<&str> = changed.iter().map(|c| c.path.as_str()).collect();
    let memory = id.entry("persona/MEMORY.md");
    ReflectionDigest {
        identity: id.slug.clone(),
        memory: memory.map(|e| e.text.clone()).unwrap_or_default(),
        memory_words: memory
            .map(|e| crate::markdown::word_count(e.body()))
            .unwrap_or(0),
        memory_changed_outside_reflection: reflection.memory_changed,
        events: of_kind(Kind::Event)
            .filter(|e| after_through(&e.field_str("date")))
            .map(Link::of)
            .collect(),
        lessons: of_kind(Kind::Lesson)
            .filter(|e| changed_paths.contains(e.rel.as_str()))
            .map(Link::of)
            .collect(),
        journal: of_kind(Kind::Journal)
            .filter(|e| after_through(&e.field_str("date")))
            .map(|e| e.rel.clone())
            .collect(),
        open_threads: open_links(id, Kind::Thread),
        open_conflicts: open_links(id, Kind::Conflict),
        recurring: recurring(&days),
        last_persona_change: persona_changelog(id, None).pop(),
        unreflected_days: days,
        changed,
        last_reflected: last,
        reflected_through: reflection.reflected_through.map(|d| d.to_string()),
    }
}

#[derive(Debug, Serialize)]
pub struct ReflectionRecord {
    pub last_reflected: String,
    pub reflected_through: Option<String>,
    pub memory_hash: Option<String>,
}

pub fn done(id: &Identity, through: Option<NaiveDate>) -> Result<ReflectionRecord> {
    let mut doc = id
        .read_object("persona/state.json")?
        .ok_or(crate::Fault::NoPersonaState)?;
    let through = match through {
        Some(t) => Some(t.to_string()),
        None => id
            .entries
            .iter()
            .filter(|e| matches!(e.kind, Kind::Log | Kind::Event | Kind::Journal))
            .filter(|e| !(e.kind == Kind::Log && e.is_blank()))
            .filter_map(|e| e.field_str("date"))
            .filter(|d| parse_day(d).is_ok())
            .max(),
    };
    let memory = id
        .entry("persona/MEMORY.md")
        .map(|e| crate::state::hash(&e.text));
    let record = ReflectionRecord {
        last_reflected: crate::state::now(),
        reflected_through: through,
        memory_hash: memory,
    };
    doc["reflection"] = serde_json::to_value(&record)?;
    crate::write::json(&id.dir.join("persona/state.json"), &doc)?;
    Ok(record)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaChange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Vec<String>>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl PersonaChange {
    fn of(record: &Value) -> Self {
        let mut other = record.as_object().cloned().unwrap_or_default();
        let mut take = |key: &str, is: fn(&Value) -> bool| match other.get(key) {
            Some(v) if is(v) => other.remove(key),
            _ => None,
        };
        let text = |v: Option<Value>| v.and_then(|v| v.as_str().map(str::to_string));
        Self {
            at: text(take("at", Value::is_string)),
            file: text(take("file", Value::is_string)),
            change: text(take("change", Value::is_string)),
            evidence: take("evidence", |v| {
                v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
            })
            .and_then(|v| serde_json::from_value(v).ok()),
            other,
        }
    }
}

pub struct PersonaChangeReport {
    pub record: PersonaChange,
    pub warning: Option<String>,
}

pub fn persona_change(
    id: &Identity,
    file: &str,
    change: &str,
    evidence: &[String],
) -> Result<PersonaChangeReport> {
    let file = file.trim_start_matches("persona/");
    if !crate::identity::PERSONA_FILES.contains(&file) {
        bail!(
            "{file:?} is not a persona file ({})",
            crate::identity::PERSONA_FILES.join(", ")
        );
    }
    if change.trim().is_empty() {
        return Err(crate::Fault::Empty("change").into());
    }
    let mut links = vec![];
    let mut seen = std::collections::BTreeSet::new();
    let mut defining = false;
    for raw in evidence {
        let name = raw.trim().trim_start_matches("[[").trim_end_matches("]]");
        let target = name.split(['#', '|']).next().unwrap_or_default().trim();
        let i = crate::find::one(id, target).context("evidence")?;
        let e = &id.entries[i];
        defining |= e.kind == Kind::Event && e.field("defining") == Some(&Value::Bool(true));
        if seen.insert(i) {
            links.push(format!("[[{name}]]"));
        }
    }
    let warning = (links.len() < 2 && !defining)
        .then(|| "persona changes need two evidence links or one defining event".to_string());
    let mut doc = id
        .read_object("persona/state.json")?
        .ok_or(crate::Fault::NoPersonaState)?;
    let recorded = doc["changelog"].as_array().is_some_and(|h| {
        h.iter()
            .any(|r| r["file"] == file && r["change"] == change.trim())
    });
    if recorded {
        bail!("{file} already records {:?}", change.trim());
    }
    let record = PersonaChange {
        at: Some(crate::state::now()),
        file: Some(file.to_string()),
        change: Some(change.trim().to_string()),
        evidence: Some(links),
        other: Map::new(),
    };
    let value = serde_json::to_value(&record)?;
    match doc.get_mut("changelog").and_then(Value::as_array_mut) {
        Some(h) => h.push(value),
        None => doc["changelog"] = json!([value]),
    }
    crate::write::json(&id.dir.join("persona/state.json"), &doc)?;
    Ok(PersonaChangeReport { record, warning })
}

pub fn persona_changelog(id: &Identity, file: Option<&str>) -> Vec<PersonaChange> {
    let file = file.map(|f| f.trim_start_matches("persona/"));
    crate::state::changelog(id)
        .iter()
        .map(PersonaChange::of)
        .filter(|r| file.is_none_or(|f| r.file.as_deref() == Some(f)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recurring_phrases_skip_function_words() {
        let days: Vec<LogDay> = ["01", "02", "03"]
            .iter()
            .map(|d| LogDay {
                path: String::new(),
                date: format!("2026-01-{d}"),
                title: String::new(),
                body: "Out on the water, glazed blue tiles again.".into(),
            })
            .collect();
        let phrases: Vec<String> = recurring(&days).into_iter().map(|(p, _)| p).collect();
        assert!(phrases.contains(&"glazed blue tiles".to_string()));
        assert!(!phrases
            .iter()
            .any(|p| p == "on the water" || p == "out on the"));
    }
}
