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
    let slug = slugify(req.title);
    if slug.is_empty() && req.kind != "log" {
        bail!("title {:?} has no slug characters", req.title);
    }
    let mut values: BTreeMap<&str, String> = BTreeMap::new();
    values.insert("title", req.title.trim().to_string());
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
        let list = req
            .aliases
            .iter()
            .map(|a| format!("\"{}\"", a.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ");
        text = text.replacen("aliases: []", &format!("aliases: [{list}]"), 1);
    }
    fs::create_dir_all(path.parent().expect("has parent"))?;
    if req.kind == "project" {
        fs::create_dir_all(path.parent().expect("has parent").join("notes"))?;
    }
    fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(Added::Created(rel))
}
