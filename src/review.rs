//! `owo review`: the review loop's bookkeeping.
//! `status` lists what is unreviewed; `done` records a finished review.

use crate::identity::{Identity, Kind};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;

/// Records a review: last_reviewed = now, reviewed_through = newest log date,
/// memory_hash = current MEMORY.md hash.
pub fn done(id: &Identity) -> Result<Value> {
    let mut doc = id
        .read_json("soul/state.json")
        .context("soul/state.json missing; run `owo stamp`")?;
    let through = id
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Log)
        .filter_map(|e| e.field_str("date"))
        .max();
    let memory = id
        .entry("soul/MEMORY.md")
        .map(|e| crate::state::hash(&e.text));
    let review = json!({ "last_reviewed": crate::state::now(), "reviewed_through": through, "memory_hash": memory });
    doc["review"] = review.clone();
    fs::write(
        id.dir.join("soul/state.json"),
        format!("{}\n", serde_json::to_string_pretty(&doc)?),
    )?;
    Ok(review)
}
