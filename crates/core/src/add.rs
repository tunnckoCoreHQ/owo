use crate::identity::{Identity, Kind};
use crate::slug::slugify;
use crate::templates;
use crate::write::{self, Writes};
use crate::Fault;
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;

use crate::AddRequest as Request;

pub fn add(id: &Identity, req: &Request, project: Option<&str>) -> Result<(String, bool)> {
    let name = req
        .kind
        .type_name()
        .expect("`Identity::add` takes only entry types");
    let (parent, wiki) = (req.parent.is_some(), req.wiki.is_some());
    for (set, field, noun, kind) in [
        (parent, "parent", "parent", Kind::LocationDetail),
        (wiki, "wiki", "project wiki path", Kind::ProjectReadme),
    ] {
        if set && req.kind != kind {
            return Err(Fault::OnlyFor {
                field,
                noun,
                kind,
                given: req.kind,
            }
            .into());
        }
    }
    let today = crate::state::local_today();
    let date = req
        .date
        .map(|d| d.to_string())
        .unwrap_or_else(|| today.clone());
    if req.kind == Kind::DailyLog {
        let day = format!("life/daily-log/{date}_");
        if let Some(e) = id
            .entries
            .iter()
            .find(|e| e.kind == Kind::DailyLog && e.rel.starts_with(&day))
        {
            return Ok((e.rel.clone(), false));
        }
    }
    let title = req.title.trim();
    if title.is_empty() {
        return Err(Fault::NeedsTitle(req.kind).into());
    }
    for name in std::iter::once(title).chain(req.aliases.iter().map(String::as_str)) {
        if name.contains(crate::markdown::WIKILINK_BREAKERS) {
            bail!("{name:?} holds # | [ or ], which break a [[wikilink]] to it");
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
        let parent = req.parent.as_deref().ok_or(Fault::KindNeeds {
            kind: req.kind,
            field: "parent",
        })?;
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

    if req.kind == Kind::ProjectNote {
        let project = project.ok_or(Fault::KindNeeds {
            kind: req.kind,
            field: "project",
        })?;
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
    let builtin = req
        .kind
        .template()
        .expect("every entry type has a template");
    let root = id.dir.parent().unwrap_or(&id.dir);
    let mut text = templates::fill(
        &templates::get(root, &format!("{name}.md"), builtin),
        &values,
    );
    let fill_line = |text: &str, from: &'static str, to: &str, field| {
        if !text.contains(from) {
            let line = from.trim_start_matches('\n');
            return Err(Fault::TemplateLacks {
                template: name.into(),
                line,
                field,
            });
        }
        Ok(text.replacen(from, to, 1))
    };
    if !req.aliases.is_empty() {
        let list = req
            .aliases
            .iter()
            .map(|a| format!("\n  - \"{}\"", templates::yaml_escape(a)))
            .collect::<String>();
        text = fill_line(
            &text,
            "\naliases: []",
            &format!("\naliases:{list}"),
            "alias",
        )?;
    }
    if let (Some(wiki), Kind::ProjectReadme) = (req.wiki.as_deref(), req.kind) {
        let to = format!("wiki: \"{}\"", templates::yaml_escape(wiki));
        text = fill_line(&text, "wiki: \"\"", &to, "wiki")?;
    }
    let notes = path.with_file_name("notes");
    let writes = Writes::default().file(&path, &path, text);
    match req.kind {
        Kind::ProjectReadme => writes.dir(&notes, &notes),
        _ => writes,
    }
    .commit()?;
    Ok((rel, true))
}

pub fn write(id: &Identity, rel: &str, text: &str) -> Result<bool> {
    if text.trim().is_empty() {
        return Err(Fault::NoText(rel.into()).into());
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
    let scoped = ["persona/", "life/", "work/"]
        .iter()
        .any(|s| clean.starts_with(s));
    if !scoped || !clean.ends_with(".md") {
        bail!("{rel:?} must be an identity-relative .md path under persona/, life/ or work/");
    }
    if crate::identity::classify(&clean) == crate::identity::Kind::Generated {
        return Err(Fault::Generated(clean).into());
    }
    let file = id.dir.join(&clean);
    let created = !file.exists();
    write::file(&file, text)?;
    Ok(created)
}
