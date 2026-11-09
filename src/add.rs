//! `owo add`: creates one entry from its template at its deterministic path.

use crate::identity::Identity;
use crate::slug::slugify;
use crate::templates;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;

pub struct Request<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    pub date: Option<&'a str>,
    pub parent: Option<&'a str>,
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
    Some(match kind {
        "character" => "characters",
        "culture" => "cultures",
        "faction" => "factions",
        "event" => "events",
        "era" => "eras",
        "thread" => "threads",
        "conflict" => "conflicts",
        "lesson" => "lessons",
        "writing" => "writing",
        "journal" => "journal",
        "log" => "log",
        _ => return None,
    })
}

pub fn add(id: &Identity, req: &Request) -> Result<Added> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let date = req
        .date
        .map(str::to_string)
        .unwrap_or_else(|| today.clone());
    if chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_err() {
        bail!("--date {date:?} is not YYYY-MM-DD");
    }
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
            let project = slugify(project);
            if id
                .entry(&format!("work/projects/{project}/project.md"))
                .is_none()
            {
                bail!(
                    "project not found: work/projects/{project}/project.md (run `owo add project`)"
                );
            }
            values.insert("project", project.clone());
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
