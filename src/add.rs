//! `owo add`: creates one entry from its template at its deterministic path.

use crate::identity::{Identity, LIFE_CATEGORIES};
use crate::slug::slugify;
use crate::templates;
use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use std::collections::BTreeMap;
use std::fs;

pub struct Request<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub date: Option<NaiveDate>,
    pub parent: Option<&'a str>,
    /// Project slug for a note, already checked to exist
    pub project: Option<&'a str>,
    pub aliases: &'a [String],
    /// KB path for a project (`kb:` in project.md)
    pub kb: Option<&'a str>,
}

pub enum Added {
    Created(String),
    Exists(String),
}

fn folder(kind: &str) -> Option<&'static str> {
    LIFE_CATEGORIES
        .iter()
        .find(|(_, k)| k.type_name() == Some(kind))
        .map(|(dir, _)| *dir)
}

pub fn add(id: &Identity, req: &Request) -> Result<Added> {
    if req.parent.is_some() && req.kind != "location-detail" {
        bail!(
            "--parent is for location-detail; a {} has no parent",
            req.kind
        );
    }
    if req.kb.is_some() && req.kind != "project" {
        bail!("--kb is for project; a {} has no KB", req.kind);
    }
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let date = req
        .date
        .map(|d| d.to_string())
        .unwrap_or_else(|| today.clone());
    // A log without a title is titled by its date, so titles stay unique.
    let title = match req.title.trim() {
        "" if req.kind == "log" => date.as_str(),
        t => t,
    };
    let slug = slugify(title);
    if slug.is_empty() {
        bail!("title {title:?} has no slug characters; pass a title");
    }
    let mut values: BTreeMap<&str, String> = BTreeMap::new();
    values.insert("title", title.to_string());
    values.insert("today", today);
    values.insert("date", date.clone());

    let rel = match req.kind {
        "location" => format!("life/locations/{slug}/overview.md"),
        "location-detail" => {
            let parent = req
                .parent
                .context("location-detail needs --parent <location>")?;
            let parent_slug = slugify(parent);
            let overview = format!("life/locations/{parent_slug}/overview.md");
            let parent_entry = id
                .entry(&overview)
                .with_context(|| format!("parent location not found: {overview}"))?;
            values.insert(
                "parent",
                parent_entry
                    .title
                    .clone()
                    .unwrap_or_else(|| parent.to_string()),
            );
            format!("life/locations/{parent_slug}/{slug}.md")
        }
        "project" => format!("work/projects/{slug}/project.md"),
        "note" => {
            let project = req.project.context("note needs --project <project>")?;
            values.insert("project", project.to_string());
            format!("work/projects/{project}/notes/{date}_{slug}.md")
        }
        "log" => format!("life/log/{date}.md"),
        "journal" | "writing" => {
            format!("life/{}/{date}_{slug}.md", folder(req.kind).expect("known"))
        }
        other => match folder(other) {
            Some(dir) => format!("life/{dir}/{slug}.md"),
            None => bail!("unknown entry type {other:?}; run `owo templates` for the list"),
        },
    };

    let path = id.dir.join(&rel);
    if path.exists() {
        return Ok(Added::Exists(rel));
    }
    let builtin = templates::ENTRY_TEMPLATES
        .iter()
        .find(|(n, _)| *n == req.kind)
        .map(|(_, t)| *t)
        .context("missing template")?;
    let root = id.dir.parent().unwrap_or(&id.dir);
    let mut text = templates::fill(
        &templates::get(root, &format!("{}.md", req.kind), builtin),
        &values,
    );
    if !req.aliases.is_empty() {
        if !text.contains("aliases: []") {
            bail!(
                "the {} template has no `aliases: []` line for --alias",
                req.kind
            );
        }
        let list = req
            .aliases
            .iter()
            .map(|a| format!("\n  - \"{}\"", templates::yaml_escape(a)))
            .collect::<String>();
        text = text.replacen("aliases: []", &format!("aliases:{list}"), 1);
    }
    if let (Some(kb), "project") = (req.kb, req.kind) {
        if !text.contains("kb: \"\"") {
            bail!("the project template has no `kb: \"\"` line for --kb");
        }
        let kb = templates::yaml_escape(kb);
        text = text.replacen("kb: \"\"", &format!("kb: \"{kb}\""), 1);
    }
    fs::create_dir_all(path.parent().expect("has parent"))?;
    if req.kind == "project" {
        fs::create_dir_all(path.parent().expect("has parent").join("notes"))?;
    }
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(Added::Created(rel))
}

/// Writes the full text of one authored file, creating its folder. Returns true when the file
/// is new. The path stays inside soul/, life/ or work/ and is never a generated file.
pub fn write(id: &Identity, rel: &str, text: &str) -> Result<bool> {
    if text.trim().is_empty() {
        bail!("no text for {rel}; pass --content or pipe the full file on stdin");
    }
    let parts: Option<Vec<&str>> = std::path::Path::new(rel)
        .components()
        .map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    let clean = parts.map(|p| p.join("/")).unwrap_or_default();
    let scoped = ["soul/", "life/", "work/"]
        .iter()
        .any(|s| clean.starts_with(s));
    if !scoped || !clean.ends_with(".md") {
        bail!("{rel:?} must be an identity-relative .md path under soul/, life/ or work/");
    }
    if crate::identity::classify(&clean) == crate::identity::Kind::Generated {
        bail!("{clean} is generated; run `owo index` instead");
    }
    let file = id.dir.join(&clean);
    let created = !file.exists();
    fs::create_dir_all(file.parent().expect("has parent"))?;
    fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
    Ok(created)
}
