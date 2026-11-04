//! `owo export card` (Character Card V3 or V2) and `owo export soulspec` (SoulSpec v0.5).

use crate::identity::{Entry, Identity, Kind};
use crate::markdown;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Licenses `export soulspec` accepts without a warning.
pub const LICENSES: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "CC-BY-4.0",
    "CC-BY-SA-4.0",
    "CC-BY-NC-4.0",
    "CC0-1.0",
    "MPL-2.0",
    "UNLICENSED",
];

/// Body text without the first `# ` heading and without the listed `## ` sections.
fn body_without(text: &str, drop: &[&str]) -> String {
    let body = markdown::split(text).body;
    let mut out = vec![];
    let mut skipping = false;
    for line in body.lines() {
        if line.starts_with("# ") {
            continue;
        }
        if let Some(h) = line.strip_prefix("## ") {
            skipping = drop.iter().any(|d| d.eq_ignore_ascii_case(h.trim()));
        }
        if !skipping {
            out.push(line);
        }
    }
    out.join("\n").trim().to_string()
}

fn text(id: &Identity, rel: &str) -> String {
    id.entry(rel)
        .map(|e| body_without(&e.text, &[]))
        .unwrap_or_default()
}

fn section(id: &Identity, rel: &str, name: &str) -> String {
    id.entry(rel)
        .and_then(|e| markdown::section(e.body(), name))
        .map(|lines| {
            lines
                .iter()
                .map(|(_, l)| *l)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

/// `## ` sections of a file as separate strings (greetings, one per section).
fn sections(id: &Identity, rel: &str) -> Vec<String> {
    let body = id
        .entry(rel)
        .map(|e| body_without(&e.text, &[]))
        .unwrap_or_default();
    body.split("\n## ")
        .map(|s| s.trim_start_matches("## "))
        .filter_map(|s| s.split_once('\n').map(|(_, rest)| rest.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

fn order(kind: Kind) -> Option<i64> {
    Some(match kind {
        Kind::Character => 20,
        Kind::Location | Kind::LocationDetail => 30,
        Kind::Culture | Kind::Faction => 40,
        Kind::Era | Kind::Event => 50,
        Kind::Lesson => 60,
        Kind::Project => 70,
        _ => return None,
    })
}

fn lore(e: &Entry, n: usize) -> Value {
    let title = e.title.clone().unwrap_or_else(|| e.stem_key());
    let mut keys = vec![title.clone()];
    keys.extend(e.aliases.iter().cloned());
    let summary = e.field_str("summary").unwrap_or_default();
    let body = body_without(&e.text, &["Identifiers", "Sources"]);
    let content = [summary, body]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    json!({
        "id": n, "keys": keys, "content": content, "extensions": {}, "enabled": true,
        "insertion_order": order(e.kind).unwrap_or(90), "case_sensitive": false, "use_regex": false,
        "constant": false, "name": title, "priority": 10, "comment": e.rel
    })
}

pub fn card(id: &Identity, spec: &str, project: Option<&str>) -> Result<Value> {
    let name = id
        .manifest
        .as_ref()
        .and_then(|m| m["name"].as_str())
        .unwrap_or(&id.slug)
        .to_string();
    let soul_json = id.read_json("soul/soul.json").unwrap_or(Value::Null);
    let mut description = text(id, "soul/IDENTITY.md");
    let look = text(id, "soul/LOOK.md");
    if !look.is_empty() {
        description.push_str(&format!("\n\n## Look\n\n{look}"));
    }
    let who = section(id, "soul/IDENTITY.md", "Who she is")
        + &section(id, "soul/IDENTITY.md", "Who he is")
        + &section(id, "soul/IDENTITY.md", "Who they are");
    let mut system_prompt = if who.is_empty() {
        text(id, "soul/IDENTITY.md")
    } else {
        who
    };
    let voice = text(id, "soul/VOICE.md");
    if !voice.is_empty() {
        system_prompt.push_str(&format!("\n\nVoice: {voice}"));
    }
    let good = text(id, "soul/examples/good.md");
    let mes_example = if good.is_empty() || good.starts_with("<START>") {
        good
    } else {
        format!("<START>\n{good}")
    };
    let mut greetings = sections(id, "soul/examples/greetings.md").into_iter();
    let first_mes = greetings.next().unwrap_or_default();
    let mut scenario = String::new();
    let mut entries = vec![json!({
        "id": 0, "keys": ["memory"], "content": text(id, "soul/MEMORY.md"), "extensions": {}, "enabled": true,
        "insertion_order": 10, "case_sensitive": false, "use_regex": false, "constant": true, "name": "Memory", "priority": 0, "comment": "soul/MEMORY.md"
    })];
    let project_prefix = project.map(|p| format!("work/projects/{p}/"));
    if let Some(p) = project {
        let Some(e) = id.entry(&format!("work/projects/{p}/project.md")) else {
            bail!("project not found: work/projects/{p}/project.md")
        };
        scenario = format!(
            "{}\n\n{}",
            e.field_str("summary").unwrap_or_default(),
            body_without(&e.text, &["Sources"])
        )
        .trim()
        .to_string();
    }
    for e in &id.entries {
        let include = match e.kind {
            Kind::Project | Kind::Note => {
                project_prefix
                    .as_ref()
                    .is_some_and(|p| e.rel.starts_with(p))
                    && e.kind == Kind::Project
            }
            k => order(k).is_some(),
        };
        if include {
            entries.push(lore(e, entries.len()));
        }
    }
    let tags = soul_json.get("tags").cloned().unwrap_or(json!([]));
    let creator = soul_json
        .pointer("/author/name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let version = soul_json
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("1.0.0")
        .to_string();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut data = json!({
        "name": name,
        "description": description,
        "personality": text(id, "soul/SOUL.md"),
        "scenario": scenario,
        "first_mes": first_mes,
        "mes_example": mes_example,
        "creator_notes": format!("Exported by owo {} from identity {} (owo/1) on {today}.", env!("CARGO_PKG_VERSION"), id.slug),
        "system_prompt": system_prompt,
        "post_history_instructions": "",
        "alternate_greetings": greetings.collect::<Vec<_>>(),
        "tags": tags,
        "creator": creator,
        "character_version": version,
        "extensions": { "owo": { "identity": id.slug, "project": project } },
        "character_book": {
            "name": format!("{name} lore"), "description": "", "scan_depth": 4, "token_budget": 1024,
            "recursive_scanning": false, "extensions": {}, "entries": entries
        }
    });
    if spec == "v2" {
        return Ok(json!({ "spec": "chara_card_v2", "spec_version": "2.0", "data": data }));
    }
    let now = chrono::Utc::now().timestamp();
    let nickname = id
        .entry("soul/IDENTITY.md")
        .and_then(|e| e.aliases.first().cloned());
    let extra = json!({
        "nickname": nickname, "assets": [{ "type": "icon", "uri": "ccdefault:", "name": "main", "ext": "png" }],
        "creator_notes_multilingual": {}, "source": [], "group_only_greetings": [],
        "creation_date": now, "modification_date": now
    });
    for (k, v) in extra.as_object().expect("object") {
        if !v.is_null() {
            data[k] = v.clone();
        }
    }
    Ok(json!({ "spec": "chara_card_v3", "spec_version": "3.0", "data": data }))
}

pub struct Report {
    pub files: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn soulspec(id: &Identity, dir: &Path) -> Result<Report> {
    let soul = id.dir.join("soul");
    let mut manifest = id.read_json("soul/soul.json").unwrap_or_else(|| json!({}));
    let mut warnings = vec![];
    let license = manifest["license"].as_str().unwrap_or("").to_string();
    if !LICENSES.contains(&license.as_str()) {
        warnings.push(format!(
            "license {license:?} is outside the SoulSpec allowlist ({})",
            LICENSES.join(", ")
        ));
    }
    manifest["files"]["style"] = json!("STYLE.md");
    manifest["files"]["memory"] = json!("MEMORY.md");
    fs::create_dir_all(dir.join("examples"))?;
    let mut files = vec![];
    let mut write = |rel: &str, body: String| -> Result<()> {
        fs::write(dir.join(rel), body)?;
        files.push(rel.to_string());
        Ok(())
    };
    write(
        "soul.json",
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    let read = |rel: &str| fs::read_to_string(soul.join(rel)).unwrap_or_default();
    write("SOUL.md", read("SOUL.md"))?;
    let look = text(id, "soul/LOOK.md");
    let mut identity = read("IDENTITY.md").trim_end().to_string();
    if !look.is_empty() {
        identity.push_str(&format!("\n\n## Look\n\n{look}"));
    }
    write("IDENTITY.md", format!("{identity}\n"))?;
    write(
        "STYLE.md",
        read("VOICE.md").replacen("# Voice", "# Style", 1),
    )?;
    write("MEMORY.md", read("MEMORY.md"))?;
    for name in ["good.md", "bad.md", "greetings.md"] {
        if soul.join("examples").join(name).is_file() {
            write(
                &format!("examples/{name}"),
                read(&format!("examples/{name}")),
            )?;
        }
    }
    Ok(Report { files, warnings })
}
