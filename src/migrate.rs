//! `owo migrate v1 <path> --out <root>/<slug>`: the mechanical part of moving a v1
//! identity (a `references/` tree) into the owo layout. Prints a checklist for the rest.

use crate::slug::slugify;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use serde_yaml::{Mapping, Value as Yaml};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Default)]
pub struct Report {
    pub identity: String,
    pub dir: String,
    pub moved: Vec<(String, String)>,
    pub skipped: Vec<String>,
    pub checklist: Vec<String>,
}

const SOUL: &[(&str, &str)] = &[
    ("IDENTITY.md", "IDENTITY.md"),
    ("SOUL.md", "SOUL.md"),
    ("LOOK.md", "LOOK.md"),
    ("MEMORY.md", "MEMORY.md"),
    ("STYLE.md", "VOICE.md"),
];

fn date_prefix(stem: &str) -> Option<String> {
    let d = stem.get(..10)?;
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
        .ok()
        .map(|_| d.to_string())
}

fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|s| s.trim().to_string())
}

/// Adds the owo frontmatter fields a v1 file lacks; keeps every existing field.
fn normalize(text: &str, kind: &str, stem: &str, date: Option<&str>, today: &str) -> String {
    let split = crate::markdown::split(text);
    let mut map: Mapping = split
        .frontmatter
        .and_then(|fm| serde_yaml::from_str(fm).ok())
        .unwrap_or_default();
    let title = map
        .get("title")
        .and_then(Yaml::as_str)
        .map(str::to_string)
        .or_else(|| first_heading(split.body))
        .unwrap_or_else(|| stem.replace('-', " "));
    let mut set = |k: &str, v: Yaml| {
        if !map.contains_key(k) {
            map.insert(Yaml::from(k), v);
        }
    };
    set("type", Yaml::from(kind));
    set("title", Yaml::from(title.as_str()));
    set("aliases", Yaml::Sequence(vec![]));
    set("summary", Yaml::from(""));
    set("status", Yaml::from("canon"));
    set("created", Yaml::from(date.unwrap_or(today)));
    set("updated", Yaml::from(today));
    if let Some(d) = date {
        set("date", Yaml::from(d));
    }
    match kind {
        "character" => {
            set("pronouns", Yaml::from(""));
            set("kind", Yaml::from("person"));
        }
        "event" => {
            set("places", Yaml::Sequence(vec![]));
            set("people", Yaml::Sequence(vec![]));
        }
        "lesson" => set("evidence", Yaml::Sequence(vec![])),
        "conflict" => set("state", Yaml::from("open")),
        _ => {}
    }
    set("sources", Yaml::Sequence(vec![]));
    map.insert(Yaml::from("type"), Yaml::from(kind));
    let fm = serde_yaml::to_string(&map).unwrap_or_default();
    let body = if first_heading(split.body).is_some() {
        split.body.trim_start().to_string()
    } else {
        format!("# {title}\n\n{}", split.body.trim_start())
    };
    format!("---\n{fm}---\n\n{body}")
}

fn md_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md") && p.is_file())
        .collect();
    v.sort();
    v
}

pub fn v1(src: &Path, out: &Path) -> Result<Report> {
    let refs = src.join("references");
    if !refs.is_dir() {
        bail!(
            "{} has no references/ folder; expected a v1 identity",
            src.display()
        );
    }
    let slug = out
        .file_name()
        .and_then(|s| s.to_str())
        .context("--out must end in the identity slug")?
        .to_string();
    let root = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let manifest: Value = fs::read_to_string(refs.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(json!({}));
    let name = manifest["name"]
        .as_str()
        .or(manifest["display_name"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| slug.clone());
    crate::create::new_identity(root, &name, &slug)?;
    let dir = root.join(&slug);
    let today = crate::state::now()[..10].to_string();
    let mut r = Report {
        identity: slug.clone(),
        dir: dir.display().to_string(),
        ..Default::default()
    };
    let rel = |p: &Path| p.strip_prefix(src).unwrap_or(p).display().to_string();

    for (from, to) in SOUL {
        let p = refs.join("soul").join(from);
        if p.is_file() {
            fs::copy(&p, dir.join("soul").join(to))?;
            r.moved.push((rel(&p), format!("soul/{to}")));
        }
    }
    if let Some(old) = fs::read_to_string(refs.join("soul/state.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        let path = dir.join("soul/state.json");
        let mut doc: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
        if let Some(h) = old.get("history").filter(|h| h.is_array()) {
            doc["history"] = h.clone();
        }
        fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&doc)?))?;
        r.moved.push((
            "references/soul/state.json".into(),
            "soul/state.json (history)".into(),
        ));
    }
    {
        let path = dir.join("identity.json");
        let mut doc: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
        if let Some(acc) = manifest
            .get("accounts")
            .filter(|a| a.is_array() || a.is_object())
        {
            doc["accounts"] = acc.clone();
        }
        fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&doc)?))?;
        if refs.join("manifest.json").is_file() {
            r.moved
                .push(("references/manifest.json".into(), "identity.json".into()));
        }
    }

    let world = refs.join("world");
    let groups: &[(&str, &str, &str, bool)] = &[
        ("characters", "life/characters", "character", false),
        ("history/events", "life/events", "event", true),
        ("memory/daily", "life/log", "log", true),
        ("memory/journal", "life/journal", "journal", true),
        ("memory/lessons", "life/lessons", "lesson", false),
    ];
    for (from, to, kind, dated) in groups {
        for p in md_files(&world.join(from)) {
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let date = date_prefix(&stem);
            if *dated && date.is_none() && *kind != "event" {
                r.skipped
                    .push(format!("{} (no date in the file name)", rel(&p)));
                continue;
            }
            let text = fs::read_to_string(&p)?;
            let date = date.or_else(|| {
                let fm = crate::markdown::split(&text).frontmatter?;
                let m: Mapping = serde_yaml::from_str(fm).ok()?;
                m.get("date").and_then(Yaml::as_str).map(str::to_string)
            });
            if *kind == "event" && date.is_none() {
                r.skipped
                    .push(format!("{} (event without a date)", rel(&p)));
                continue;
            }
            let rest = match &date {
                Some(d) if stem.starts_with(d.as_str()) => {
                    stem[d.len()..].trim_start_matches(['_', '-']).to_string()
                }
                _ => stem.clone(),
            };
            let label = if rest.is_empty() {
                date.clone().unwrap_or_else(|| "Day".into())
            } else {
                rest.clone()
            };
            let name = match *kind {
                "log" => format!("{}.md", date.clone().unwrap_or_default()),
                "journal" => format!(
                    "{}_{}.md",
                    date.clone().unwrap_or_default(),
                    slugify(if rest.is_empty() { "entry" } else { &rest })
                ),
                _ => format!("{}.md", slugify(&rest)),
            };
            let target = dir.join(to).join(&name);
            fs::write(
                &target,
                normalize(
                    &text,
                    kind,
                    &label,
                    date.as_deref().filter(|_| *dated),
                    &today,
                ),
            )?;
            r.moved.push((rel(&p), format!("{to}/{name}")));
        }
    }
    if let Ok(text) = fs::read_to_string(world.join("meta/conflicts.md")) {
        for block in text.split("\n## ").skip(1) {
            let (title, body) = block.split_once('\n').unwrap_or((block, ""));
            let slug = slugify(title);
            if slug.is_empty() {
                continue;
            }
            let file = format!("life/conflicts/{slug}.md");
            let doc = format!(
                "# {}\n\n## Account A\n\n{}\n\n## Account B\n\n## Resolution\n",
                title.trim(),
                body.trim()
            );
            fs::write(
                dir.join(&file),
                normalize(&doc, "conflict", &slug, None, &today),
            )?;
            r.moved
                .push(("references/world/meta/conflicts.md".into(), file));
        }
    }
    if world.join("meta/changelog.md").is_file() {
        r.skipped
            .push("references/world/meta/changelog.md (Git history is the change log)".into());
    }
    let sources = md_files(&src.join("sources"));
    if !sources.is_empty() {
        let to = root.join("sources").join(&slug);
        fs::create_dir_all(&to)?;
        for p in sources {
            let name = p.file_name().expect("file").to_owned();
            fs::copy(&p, to.join(&name))?;
            r.moved.push((
                rel(&p),
                format!("<root>/sources/{slug}/{}", name.to_string_lossy()),
            ));
        }
    }
    let known: Vec<String> = r.moved.iter().map(|(f, _)| f.clone()).collect();
    for e in walkdir::WalkDir::new(&refs)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
    {
        let p = rel(e.path());
        if !known.contains(&p) && !r.skipped.iter().any(|s| s.starts_with(&p)) {
            r.skipped.push(format!("{p} (no owo home; move by hand)"));
        }
    }

    let id = crate::identity::Identity::load(root, &slug)?;
    crate::index::write(&id)?;
    let id = crate::identity::Identity::load(root, &slug)?;
    crate::stamp::run(&id, false)?;
    r.checklist = vec![
        "Move MEMORY \"Moments\" into event links under \"Defining moments\".".into(),
        "Move open promises and plans into life/threads/ (`owo add thread`).".into(),
        "Replace repeated accounts and quotes in logs with event links (`owo lint --rule dupes`).".into(),
        "Move exact personal identifiers into each character's `## Identifiers` (`owo lint --rule identifiers`).".into(),
        "Name account kinds in IDENTITY.md and keep handles in identity.json accounts.".into(),
        "Fill summaries, and give locations folders with overview.md (`owo add location`).".into(),
        "Run `owo stamp && owo index && owo lint`, then the first review (`owo review`, `owo review done`).".into(),
    ];
    Ok(r)
}
