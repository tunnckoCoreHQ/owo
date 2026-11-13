//! `owo add`: creates one entry from its template at its deterministic path.

use crate::identity::{write_file, Identity, Kind};
use crate::slug::slugify;
use crate::templates;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;

use crate::AddRequest as Request;

/// Returns the entry path and true when the file is new.
/// `project` is the project of a note, checked to exist.
pub fn add(id: &Identity, req: &Request, project: Option<&str>) -> Result<(String, bool)> {
    let name = req
        .kind
        .type_name()
        .expect("`Identity::add` takes only entry types");
    if req.parent.is_some() && req.kind != Kind::LocationDetail {
        bail!("--parent is for location-detail; a {name} has no parent");
    }
    if req.kb.is_some() && req.kind != Kind::Project {
        bail!("--kb is for project; a {name} has no KB");
    }
    // The local day, so `owo add log` names the day the user is living.
    let today = crate::state::local_today();
    let date = req
        .date
        .map(|d| d.to_string())
        .unwrap_or_else(|| today.clone());
    // A log without a title is titled by its date, so titles stay unique.
    let title = match req.title.trim() {
        "" if req.kind == Kind::Log => date.as_str(),
        t => t,
    };
    for name in std::iter::once(title).chain(req.aliases.iter().map(String::as_str)) {
        if name.contains(crate::markdown::LINK_BREAKERS) {
            bail!("{name:?} holds # | [ or ], which break a [[link]] to it");
        }
    }
    let slug = slugify(title);
    if slug.is_empty() {
        bail!("title {title:?} has no slug characters; pass a title");
    }
    let mut values: BTreeMap<&str, String> = BTreeMap::new();
    values.insert("title", title.to_string());
    values.insert("today", today);
    values.insert("date", date.clone());

    let parent_slug = if req.kind == Kind::LocationDetail {
        let parent = req
            .parent
            .as_deref()
            .context("location-detail needs --parent <location>")?;
        let parent_slug = slugify(parent);
        let overview = Kind::Location
            .rel(&parent_slug, &date, None, None)
            .expect("a location path");
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
        Some(parent_slug)
    } else {
        None
    };

    if req.kind == Kind::Note {
        let project = project.context("note needs --project <project>")?;
        values.insert("project", project.to_string());
    }

    let rel = req
        .kind
        .rel(&slug, &date, parent_slug.as_deref(), project)
        .expect("every entry type has an add path");

    let path = id.dir.join(&rel);
    if path.exists() {
        return Ok((rel, false));
    }
    let builtin = templates::ENTRY_TEMPLATES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .expect("every entry type has a template");
    let root = id.dir.parent().unwrap_or(&id.dir);
    let mut text = templates::fill(
        &templates::get(root, &format!("{name}.md"), builtin),
        &values,
    );
    if !req.aliases.is_empty() {
        if !text.contains("\naliases: []") {
            bail!("the {name} template has no `aliases: []` line for --alias");
        }
        let list = req
            .aliases
            .iter()
            .map(|a| format!("\n  - \"{}\"", templates::yaml_escape(a)))
            .collect::<String>();
        text = text.replacen("\naliases: []", &format!("\naliases:{list}"), 1);
    }
    if let (Some(kb), Kind::Project) = (req.kb.as_deref(), req.kind) {
        if !text.contains("kb: \"\"") {
            bail!("the project template has no `kb: \"\"` line for --kb");
        }
        let kb = templates::yaml_escape(kb);
        text = text.replacen("kb: \"\"", &format!("kb: \"{kb}\""), 1);
    }
    write_file(&path, text)?;
    if req.kind == Kind::Project {
        fs::create_dir_all(path.with_file_name("notes"))?;
    }
    Ok((rel, true))
}

/// Writes the full text of one authored file, creating its folder. Returns true when the file
/// is new. The path stays inside soul/, life/ or work/ and is never a generated file.
pub fn write(id: &Identity, rel: &str, text: &str) -> Result<bool> {
    if text.trim().is_empty() {
        bail!("no text for {rel}; pass --content or pipe the full file on stdin");
    }
    let parts: Option<Vec<&str>> = std::path::Path::new(rel)
        .components()
        .filter(|c| *c != std::path::Component::CurDir)
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
        return Err(crate::Fault::Generated(clean).into());
    }
    let file = id.dir.join(&clean);
    let created = !file.exists();
    write_file(&file, text)?;
    Ok(created)
}
