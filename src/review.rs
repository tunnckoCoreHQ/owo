//! `owo review` (the packet the review skill compiles MEMORY from), `owo review done`,
//! and the soul history: `owo soul note`, `owo soul history`.

use crate::identity::{write_json, Identity, Kind};
use crate::state::parse_day;
use crate::status::Link;
use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Changed {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub summary: Option<String>,
    pub words: usize,
    pub updated: String,
    /// Lines added and removed in Git since the last review, when the root is a repo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<(usize, usize)>,
}

#[derive(Debug, Serialize)]
pub struct Day {
    pub path: String,
    pub date: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Serialize)]
pub struct Packet {
    pub identity: String,
    pub last_reviewed: Option<String>,
    pub reviewed_through: Option<String>,
    pub memory: String,
    pub memory_words: usize,
    pub memory_changed_outside_review: bool,
    pub changed: Vec<Changed>,
    pub unreviewed_days: Vec<Day>,
    pub events: Vec<Link>,
    pub lessons: Vec<Link>,
    pub journal: Vec<String>,
    pub open_threads: Vec<Link>,
    pub open_conflicts: Vec<Link>,
    pub recurring: Vec<(String, usize)>,
    pub last_soul_change: Option<Value>,
}

/// Short function words; a recurring phrase may not start or end with one.
const STOP_WORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "from", "had", "has", "have",
    "he", "her", "his", "if", "in", "into", "is", "it", "its", "me", "my", "of", "on", "or", "our",
    "she", "so", "that", "the", "their", "them", "then", "they", "this", "to", "was", "we", "were",
    "with", "you", "your",
];

/// Three-word phrases that show up on at least three different log days and start and end
/// with a content word.
fn recurring(days: &[Day]) -> Vec<(String, usize)> {
    let content = |w: &str| !STOP_WORDS.contains(&w);
    let mut seen: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    // A phrase stays inside one line, so two bullets never join into one.
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

pub fn packet(id: &Identity) -> Packet {
    let review = crate::state::review(id);
    let last = review.last_reviewed.clone();
    let after_through = |d: &Option<String>| review.unreviewed(d.as_deref());
    let git = crate::git::is_repo(&id.dir);
    let mut changed = vec![];
    for e in crate::state::tracked(id) {
        let updated = crate::state::updated(id, &e.rel)
            .unwrap_or_default()
            .to_string();
        // A file with no stamp yet is new since the last stamp, so it counts as changed.
        let reviewed =
            !updated.is_empty() && last.as_deref().is_some_and(|l| updated.as_str() <= l);
        if reviewed || e.rel == "soul/MEMORY.md" {
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
    let mut days: Vec<Day> = id
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Log && !e.is_blank() && after_through(&e.field_str("date")))
        .map(|e| Day {
            path: e.rel.clone(),
            date: e.field_str("date").unwrap_or_default(),
            title: e.title.clone().unwrap_or_default(),
            // The `###` heading carries the title, so the H1 is left out.
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
    let open = |k: Kind| {
        of_kind(k)
            .filter(|e| e.field_str("state").as_deref() == Some("open"))
            .map(Link::of)
            .collect()
    };
    let changed_paths: BTreeSet<&str> = changed.iter().map(|c| c.path.as_str()).collect();
    let memory = id.entry("soul/MEMORY.md");
    Packet {
        identity: id.slug.clone(),
        memory: memory.map(|e| e.text.clone()).unwrap_or_default(),
        memory_words: memory
            .map(|e| crate::markdown::word_count(e.body()))
            .unwrap_or(0),
        memory_changed_outside_review: review.memory_changed,
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
        open_threads: open(Kind::Thread),
        open_conflicts: open(Kind::Conflict),
        recurring: recurring(&days),
        last_soul_change: crate::state::history(id).last().cloned(),
        unreviewed_days: days,
        changed,
        last_reviewed: last,
        reviewed_through: review.reviewed_through.map(|d| d.to_string()),
    }
}

/// Records a review: last_reviewed = now, reviewed_through = `through` or the newest
/// story date among logs, events and journal entries, memory_hash = current MEMORY.md hash.
pub fn done(id: &Identity, through: Option<NaiveDate>) -> Result<Value> {
    let mut doc = id
        .read_object("soul/state.json")?
        .context("soul/state.json missing; run `owo stamp`")?;
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
        .entry("soul/MEMORY.md")
        .map(|e| crate::state::hash(&e.text));
    let review = json!({ "last_reviewed": crate::state::now(), "reviewed_through": through, "memory_hash": memory });
    doc["review"] = review.clone();
    write_json(&id.dir.join("soul/state.json"), &doc)?;
    Ok(review)
}

pub struct Note {
    pub record: Value,
    pub warning: Option<String>,
}

/// Appends an evidence-backed record to `soul/state.json` → `history`.
pub fn soul_note(id: &Identity, file: &str, change: &str, evidence: &[String]) -> Result<Note> {
    let file = file.trim_start_matches("soul/");
    if !crate::identity::SOUL_FILES.contains(&file) {
        bail!(
            "{file:?} is not a soul file ({})",
            crate::identity::SOUL_FILES.join(", ")
        );
    }
    if change.trim().is_empty() {
        bail!("--change is empty");
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
        .then(|| "soul changes need two evidence links or one defining event".to_string());
    let mut doc = id
        .read_object("soul/state.json")?
        .context("soul/state.json missing; run `owo stamp`")?;
    let recorded = doc["history"].as_array().is_some_and(|h| {
        h.iter()
            .any(|r| r["file"] == file && r["change"] == change.trim())
    });
    if recorded {
        bail!("{file} already records {:?}", change.trim());
    }
    let record = json!({ "at": crate::state::now(), "file": file, "change": change.trim(), "evidence": links });
    match doc.get_mut("history").and_then(Value::as_array_mut) {
        Some(h) => h.push(record.clone()),
        None => doc["history"] = json!([record.clone()]),
    }
    write_json(&id.dir.join("soul/state.json"), &doc)?;
    Ok(Note { record, warning })
}

pub fn soul_history(id: &Identity, file: Option<&str>) -> Vec<Value> {
    let file = file.map(|f| f.trim_start_matches("soul/"));
    crate::state::history(id)
        .iter()
        .filter(|r| file.is_none_or(|f| r["file"].as_str() == Some(f)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recurring_phrases_skip_function_words() {
        let days: Vec<Day> = ["01", "02", "03"]
            .iter()
            .map(|d| Day {
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
