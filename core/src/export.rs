//! `owo export card` (Character Card V3 or V2) and `owo export soulspec` (SoulSpec v0.5).

use crate::identity::{Entry, Identity, Kind};
use crate::markdown;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// The Character Card spec version of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spec {
    V2,
    /// V3, with the V2 fields too.
    V3,
}

impl Spec {
    pub const ALL: [Spec; 2] = [Spec::V2, Spec::V3];

    pub fn name(self) -> &'static str {
        match self {
            Spec::V2 => "v2",
            Spec::V3 => "v3",
        }
    }

    pub fn from_name(name: &str) -> Option<Spec> {
        Spec::ALL.into_iter().find(|s| s.name() == name)
    }
}

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

/// Body text without its `# ` headings and without the listed sections at any level.
/// Headings inside fenced code do not count.
fn body_without(text: &str, drop: &[&str]) -> String {
    let body = markdown::split(text).body;
    let mut out = vec![];
    let mut skipping: Option<usize> = None;
    for (_, line, open) in markdown::unfenced(body) {
        if let Some((level, h)) = markdown::heading(line).filter(|_| open) {
            if skipping.is_some_and(|at| level <= at) {
                skipping = None;
            }
            if skipping.is_none() && drop.iter().any(|d| d.eq_ignore_ascii_case(h)) {
                skipping = Some(level);
            }
            if level == 1 {
                continue;
            }
        }
        if skipping.is_none() {
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
        .and_then(|e| markdown::section_text(e.body(), name))
        .map(|(_, text)| text.trim().to_string())
        .unwrap_or_default()
}

/// `## ` sections of a file as separate strings (greetings, one per section).
fn sections(id: &Identity, rel: &str) -> Vec<String> {
    let body = id
        .entry(rel)
        .map(|e| body_without(&e.text, &[]))
        .unwrap_or_default();
    // Intro text before the first `## ` heading is not a greeting; headings in fences do not count.
    let mut out: Vec<String> = vec![];
    for (_, line, open) in markdown::unfenced(&body) {
        if open && line.starts_with("## ") {
            out.push(String::new());
        } else if let Some(section) = out.last_mut() {
            section.push_str(line);
            section.push('\n');
        }
    }
    out.into_iter()
        .map(|s| s.trim().to_string())
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
    let title = e.label();
    let mut keys = vec![title.clone()];
    keys.extend(e.aliases.iter().cloned());
    let summary = e.field_str("summary").unwrap_or_default();
    // Summary plus `## Key facts`; an entry whose key facts are missing or empty gives its body.
    let facts = markdown::section_text(e.body(), "Key facts")
        .map(|(_, text)| text.trim().to_string())
        .filter(|t| !t.is_empty());
    let body = facts.unwrap_or_else(|| body_without(&e.text, &["Identifiers", "Sources"]));
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

pub fn card(
    id: &Identity,
    spec: Spec,
    project: Option<&str>,
    scenario_from: Option<&str>,
) -> Result<Value> {
    let name = id.name().to_string();
    let soul_json = id.read_json("soul/soul.json").unwrap_or(Value::Null);
    let mut description = text(id, "soul/IDENTITY.md");
    let look = text(id, "soul/LOOK.md");
    if !look.is_empty() {
        description.push_str(&format!("\n\n## Look\n\n{}", markdown::demote(&look)));
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
    let project_md = project.and_then(|p| id.project(p));
    if let Some(e) = project_md {
        scenario = format!(
            "{}\n\n{}",
            e.field_str("summary").unwrap_or_default(),
            body_without(&e.text, &["Sources"])
        )
        .trim()
        .to_string();
    }
    if let Some(name) = scenario_from {
        let i = crate::find::one(id, name).map_err(|e| crate::Fault::Field {
            name: "scenario",
            error: format!("{e:#}"),
        })?;
        let e = &id.entries[i];
        scenario = e
            .field_str("summary")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| body_without(&e.text, &["Sources", "Identifiers"]));
    }
    let mut lorebook: Vec<&Entry> = id
        .entries
        .iter()
        .filter(|e| match e.kind {
            Kind::Project => project_md.is_some_and(|p| p.rel == e.rel),
            k => order(k).is_some(),
        })
        .collect();
    // Kinds in lorebook order; within a kind, path order.
    lorebook.sort_by_key(|e| order(e.kind));
    for e in lorebook {
        entries.push(lore(e, entries.len()));
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
    let today = crate::state::today();
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
    if spec == Spec::V2 {
        return Ok(json!({ "spec": "chara_card_v2", "spec_version": "2.0", "data": data }));
    }
    let now = chrono::Utc::now().timestamp();
    let nickname = id.manifest["nickname"]
        .as_str()
        .filter(|n| !n.trim().is_empty());
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

/// Warnings for a built card: a card without a greeting or an example message opens on nothing.
pub fn warnings(card: &Value) -> Vec<String> {
    ["first_mes", "mes_example"]
        .iter()
        .filter(|k| {
            card["data"][**k]
                .as_str()
                .is_none_or(|s| s.trim().is_empty())
        })
        .map(|k| format!("{k} is empty; fill soul/examples/"))
        .collect()
}

/// Writes the built `main` card to `out`: JSON, or embedded in the `png` avatar (the V2 card
/// as `chara`, plus the V3 card as `ccv3` for V3).
pub fn write_card(
    id: &Identity,
    main: &Value,
    project: Option<&str>,
    scenario: Option<&str>,
    out: &Path,
    png: Option<&Path>,
) -> Result<()> {
    let Some(png) = png else {
        return crate::identity::write_json(out, main);
    };
    let avatar = fs::read(png)?;
    let image = if main["spec"] == "chara_card_v3" {
        let v2 = card(id, Spec::V2, project, scenario)?;
        embed_png(&avatar, &[("chara", &v2), ("ccv3", main)])?
    } else {
        embed_png(&avatar, &[("chara", main)])?
    };
    fs::write(out, image)?;
    Ok(())
}

pub struct Report {
    pub files: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn soulspec(id: &Identity, dir: &Path) -> Result<Report> {
    let soul = id.dir.join("soul");
    let mut manifest = id
        .read_object("soul/soul.json")?
        .unwrap_or_else(|| json!({}));
    if !(manifest["files"].is_object() || manifest["files"].is_null()) {
        bail!("soul/soul.json `files` is not an object");
    }
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
    crate::identity::write_json(&dir.join("soul.json"), &manifest)?;
    let mut files = vec!["soul.json".to_string()];
    let mut write = |rel: &str, body: String| -> Result<()> {
        fs::write(dir.join(rel), body)?;
        files.push(rel.to_string());
        Ok(())
    };
    let read = |rel: &str| fs::read_to_string(soul.join(rel)).unwrap_or_default();
    write("SOUL.md", read("SOUL.md"))?;
    let look = text(id, "soul/LOOK.md");
    let mut identity = read("IDENTITY.md").trim_end().to_string();
    if !look.is_empty() {
        identity.push_str(&format!("\n\n## Look\n\n{}", markdown::demote(&look)));
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

/// A `SKILL.md` wrapper for hosts that load identities as skills.
pub fn skill(id: &Identity, dir: &Path) -> Result<String> {
    let name = id.name().to_string();
    let soul = id.read_json("soul/soul.json").unwrap_or(Value::Null);
    let about = soul["description"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("The {name} identity."));
    let slug = &id.slug;
    let projects = id.projects();
    let project_line = if projects.is_empty() {
        String::new()
    } else {
        format!(
            "\nProjects: {}. Add `--project <p>` when working on one.\n",
            projects
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let text = format!(
        r#"---
name: {slug}
description: "{description}"
---

# {name}

{about}

## Load

```sh
owo -i {slug} context --voice
```
{project_line}
Add `--look` when visuals matter.

## Read on demand

- `owo -i {slug} resolve <name>`, `owo -i {slug} show <name> [--section <s>]`
- `owo -i {slug} search <words>`, `owo -i {slug} links <name>`
- `owo -i {slug} state`

## Write

- New entries: `owo -i {slug} add <type> <title>`; small things: `owo -i {slug} add log`.
- Edit existing entries in place (with file tools, or `owo -i {slug} write <path>` with the full text); each fact has one home.
- MEMORY.md and soul changes happen in review.
- Session end: `owo -i {slug} stamp && owo -i {slug} index && owo -i {slug} lint --changed`.
"#,
        description = crate::templates::yaml_escape(&format!(
            "Be {name} ({}). Use when a task asks for {name}, before speaking, writing or working as them.",
            about.trim_end_matches('.')
        )),
    );
    fs::create_dir_all(dir)?;
    let path = dir.join("SKILL.md");
    fs::write(&path, text)?;
    Ok(path.display().to_string())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Writes `png` with the card embedded as tEXt chunks: `chara` (V2 JSON) always,
/// `ccv3` (V3 JSON) for V3 cards. Existing `chara`/`ccv3` chunks are replaced.
pub fn embed_png(png: &[u8], cards: &[(&str, &Value)]) -> Result<Vec<u8>> {
    const SIG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !png.starts_with(SIG) {
        bail!("not a PNG file");
    }
    let mut out = SIG.to_vec();
    let mut pos = SIG.len();
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into()?) as usize;
        let kind = &png[pos + 4..pos + 8];
        let end = pos + 12 + len;
        if end > png.len() {
            bail!("truncated PNG chunk");
        }
        let data = &png[pos + 8..pos + 8 + len];
        let ours = kind == b"tEXt" && (data.starts_with(b"chara\0") || data.starts_with(b"ccv3\0"));
        if kind == b"IEND" {
            for (key, card) in cards {
                let mut chunk = b"tEXt".to_vec();
                chunk.extend_from_slice(key.as_bytes());
                chunk.push(0);
                chunk.extend_from_slice(base64(serde_json::to_string(card)?.as_bytes()).as_bytes());
                out.extend_from_slice(&((chunk.len() - 4) as u32).to_be_bytes());
                out.extend_from_slice(&chunk);
                out.extend_from_slice(&crc32(&chunk).to_be_bytes());
            }
        }
        if !ours {
            out.extend_from_slice(&png[pos..end]);
        }
        pos = end;
        if kind == b"IEND" {
            return Ok(out);
        }
    }
    bail!("PNG has no IEND chunk")
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_and_crc() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
        assert_eq!(super::crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn body_without_drops_sections_at_any_level_outside_code() {
        let text = "# Sam\n\nSails.\n\n### Identifiers\n\n- born 1990\n\n## Notes\n\n```sh\n# keep me\n```\n";
        assert_eq!(
            super::body_without(text, &["Identifiers"]),
            "Sails.\n\n## Notes\n\n```sh\n# keep me\n```"
        );
    }
}
