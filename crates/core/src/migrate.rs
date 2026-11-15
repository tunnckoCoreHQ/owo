use crate::markdown::{first_heading, parse_frontmatter, Fields};
use crate::slug::slugify;
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
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
    crate::state::parse_day(d).ok().map(|_| d.to_string())
}

fn frontmatter(text: &str) -> Fields {
    crate::markdown::split(text)
        .frontmatter
        .and_then(|fm| parse_frontmatter(fm).ok())
        .unwrap_or_default()
}

fn frontmatter_error(text: &str) -> Option<String> {
    crate::markdown::split(text)
        .frontmatter
        .and_then(|fm| parse_frontmatter(fm).err())
        .map(|e| e.lines().next().unwrap_or_default().to_string())
}

fn title_of(text: &str, stem: &str) -> String {
    frontmatter(text)
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| first_heading(crate::markdown::split(text).body))
        .unwrap_or_else(|| stem.replace('-', " "))
}

fn take_sources(body: &str, slug: &str) -> (Vec<String>, Vec<String>, String) {
    let (mut cites, mut links, mut rest) = (vec![], vec![], vec![]);
    let mut inside = false;
    for line in body.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            inside = h.trim().eq_ignore_ascii_case("sources");
            if inside {
                continue;
            }
        }
        if !inside {
            rest.push(line);
            continue;
        }
        let Some(item) = line.trim().strip_prefix("- ") else {
            continue;
        };
        let found = crate::markdown::links(item);
        links.extend(found.iter().map(|l| match &l.section {
            Some(s) => format!("[[{}#{s}]]", l.target),
            None => format!("[[{}]]", l.target),
        }));
        let item = item.replace('`', "");
        let item = item.trim_end_matches(['.', ' ']);
        let item = match item.strip_prefix("sources/") {
            Some(path) => format!("{slug}/{path}"),
            None => item.to_string(),
        };
        if !cites.contains(&item) {
            cites.push(item);
        }
    }
    (cites, links, rest.join("\n"))
}

fn merge(map: &mut Fields, key: &str, items: Vec<String>) {
    let mut list = match map.remove(key) {
        Some(Value::Array(a)) => a,
        Some(Value::String(s)) if !s.trim().is_empty() => vec![Value::from(s)],
        _ => vec![],
    };
    for item in items.into_iter().map(Value::from) {
        if !list.contains(&item) {
            list.push(item);
        }
    }
    map.insert(key.to_string(), Value::Array(list));
}

fn normalize(
    text: &str,
    kind: &str,
    stem: &str,
    date: Option<&str>,
    parent: Option<&str>,
    today: &str,
    slug: &str,
) -> String {
    let split = crate::markdown::split(text);
    let mut map = frontmatter(text);
    let title = title_of(text, stem);
    let (cites, links, body) = take_sources(split.body, slug);
    let mut set = |k: &str, v: Value| {
        if !map.contains_key(k) {
            map.insert(k.to_string(), v);
        }
    };
    set("type", Value::from(kind));
    set("title", Value::from(title.as_str()));
    set("aliases", json!([]));
    set("summary", Value::from(""));
    set("status", Value::from("canon"));
    set("created", Value::from(date.unwrap_or(today)));
    set("updated", Value::from(today));
    if let Some(d) = date {
        set("date", Value::from(d));
    }
    match kind {
        "character" => {
            set("pronouns", Value::from(""));
            set("kind", Value::from("person"));
        }
        "event" => {
            set("places", json!([]));
            set("people", json!([]));
        }
        "conflict" | "thread" => set("state", Value::from("open")),
        "location-detail" => set(
            "parent",
            Value::from(format!("[[{}]]", parent.unwrap_or_default())),
        ),
        _ => {}
    }
    let cites: Vec<String> = match kind {
        "lesson" => {
            merge(&mut map, "evidence", links);
            cites.into_iter().filter(|c| !c.contains("[[")).collect()
        }
        _ => cites,
    };
    merge(&mut map, "sources", cites);
    map.insert("type".to_string(), Value::from(kind));
    if !matches!(
        map.get("status").and_then(Value::as_str),
        Some("canon" | "disputed" | "superseded")
    ) {
        map.insert("status".to_string(), Value::from("canon"));
    }
    let mut block_lists = serde_saphyr::SerializerOptions::default();
    block_lists.compact_list_indent = false;
    let fm = serde_saphyr::to_string_with_options(&map, block_lists).unwrap_or_default();
    let body = if first_heading(&body).is_some() {
        body.trim().to_string()
    } else {
        format!("# {title}\n\n{}", body.trim())
    };
    format!("---\n{fm}---\n\n{body}\n")
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

fn stem_of(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn migrate_locations(
    locs: &Path,
    dir: &Path,
    today: &str,
    slug: &str,
    rel: &dyn Fn(&Path) -> String,
    r: &mut Report,
) -> Result<()> {
    let mut write = |from: &Path, text: &str, to: String, kind: &str, parent: Option<&str>| {
        if let Some(e) = frontmatter_error(text) {
            r.skipped.push(format!(
                "{} (frontmatter does not parse: {e}; move by hand)",
                rel(from)
            ));
            return anyhow::Ok(());
        }
        let target = dir.join(&to);
        if crate::write::exists(&target) {
            r.skipped
                .push(format!("{} (target {to} exists; merge by hand)", rel(from)));
            return anyhow::Ok(());
        }
        let stem = from.file_stem().map(|s| s.to_string_lossy().into_owned());
        let stem = stem.unwrap_or_default();
        crate::write::file(
            &target,
            normalize(text, kind, &stem, None, parent, today, slug),
        )?;
        r.moved.push((rel(from), to));
        anyhow::Ok(())
    };
    let loose: Vec<PathBuf> = md_files(locs)
        .into_iter()
        .filter(|p| !matches!(stem_of(p).as_str(), "index" | "_index"))
        .collect();
    let mut used: Vec<PathBuf> = vec![];
    let mut folders: Vec<PathBuf> = fs::read_dir(locs)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    for folder in folders {
        let name = folder
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let slug = slugify(&name);
        if slug.is_empty() {
            continue;
        }
        let inside = md_files(&folder);
        let overview = ["overview", "index", "readme", &name]
            .iter()
            .find_map(|n| inside.iter().find(|p| stem_of(p).eq_ignore_ascii_case(n)))
            .or_else(|| loose.iter().find(|p| slugify(&stem_of(p)) == slug))
            .cloned();
        let base = format!("life/locations/{slug}");
        let (title, text) = match &overview {
            Some(p) => {
                let text = fs::read_to_string(p)?;
                used.push(p.clone());
                (title_of(&text, &stem_of(p)), text)
            }
            None => {
                let title = name.replace(['-', '_'], " ");
                let text = format!("# {title}\n");
                (title, text)
            }
        };
        let from = overview.clone().unwrap_or_else(|| folder.clone());
        write(
            &from,
            &text,
            format!("{base}/overview.md"),
            "location",
            None,
        )?;
        for p in inside.iter().filter(|p| {
            Some(*p) != overview.as_ref() && !matches!(stem_of(p).as_str(), "index" | "_index")
        }) {
            let text = fs::read_to_string(p)?;
            let to = format!("{base}/{}.md", slugify(&stem_of(p)));
            write(p, &text, to, "location-detail", Some(&title))?;
        }
    }
    for p in loose.iter().filter(|p| !used.contains(p)) {
        let text = fs::read_to_string(p)?;
        let to = format!("life/locations/{}/overview.md", slugify(&stem_of(p)));
        write(p, &text, to, "location", None)?;
    }
    Ok(())
}

pub fn v1(cwd: &Path, src: &Path, out: &Path) -> Result<Report> {
    let refs = cwd.join(src).join("references");
    if !refs.is_dir() {
        bail!(
            "{} has no references/ folder; expected a v1 identity",
            src.display()
        );
    }
    let slug = out
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(crate::Fault::OutNoSlug)?
        .to_string();
    let shown = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let root = &cwd.join(shown);
    let src = &cwd.join(src);
    let manifest: Value = fs::read_to_string(refs.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(json!({}));
    let name = manifest["name"]
        .as_str()
        .or(manifest["display_name"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| slug.clone());
    let sources = root.join("sources").join(&slug);
    let had_sources = sources.exists();
    crate::create::new_identity(root, shown, &name, &slug)?;
    let mut report = fill(src, &refs, root, &slug, &manifest).inspect_err(|_| {
        if crate::write::dry() {
            return;
        }
        let _ = fs::remove_dir_all(root.join(&slug));
        if !had_sources {
            let _ = fs::remove_dir_all(&sources);
        }
    })?;
    report.dir = shown.join(&slug).display().to_string();
    Ok(report)
}

fn fill(src: &Path, refs: &Path, root: &Path, slug: &str, manifest: &Value) -> Result<Report> {
    let dir = root.join(slug);
    let today = crate::state::today();
    let mut r = Report {
        identity: slug.to_string(),
        dir: dir.display().to_string(),
        ..Default::default()
    };
    let rel = |p: &Path| p.strip_prefix(src).unwrap_or(p).display().to_string();

    for (from, to) in SOUL {
        let p = refs.join("soul").join(from);
        if p.is_file() {
            crate::write::file(&dir.join("soul").join(to), fs::read(&p)?)?;
            r.moved.push((rel(&p), format!("soul/{to}")));
        }
    }
    if let Some(old) = fs::read_to_string(refs.join("soul/state.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        let path = dir.join("soul/state.json");
        let mut doc: Value = serde_json::from_str(&crate::write::read_to_string(&path)?)?;
        if let Some(h) = old.get("history").filter(|h| h.is_array()) {
            doc["history"] = h.clone();
        }
        crate::write::json(&path, &doc)?;
        r.moved.push((
            "references/soul/state.json".into(),
            "soul/state.json (history)".into(),
        ));
    }
    {
        let path = dir.join("identity.json");
        let mut doc: Value = serde_json::from_str(&crate::write::read_to_string(&path)?)?;
        if let Some(acc) = manifest
            .get("accounts")
            .filter(|a| a.is_array() || a.is_object())
        {
            doc["accounts"] = acc.clone();
        }
        crate::write::json(&path, &doc)?;
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
        ("cultures", "life/cultures", "culture", false),
        ("factions", "life/factions", "faction", false),
        ("eras", "life/eras", "era", false),
        ("history/eras", "life/eras", "era", false),
        ("threads", "life/threads", "thread", false),
    ];
    for (from, to, kind, dated) in groups {
        for p in md_files(&world.join(from))
            .into_iter()
            .filter(|p| !matches!(stem_of(p).as_str(), "index" | "_index"))
        {
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
            if let Some(e) = frontmatter_error(&text) {
                r.skipped.push(format!(
                    "{} (frontmatter does not parse: {e}; move by hand)",
                    rel(&p)
                ));
                continue;
            }
            let date = date.or_else(|| {
                frontmatter(&text)
                    .get("date")
                    .and_then(Value::as_str)
                    .map(str::to_string)
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
                _ if !slugify(&rest).is_empty() => format!("{}.md", slugify(&rest)),
                _ => format!("{}.md", slugify(&title_of(&text, &label))),
            };
            let target = dir.join(to).join(&name);
            if crate::write::exists(&target) {
                r.skipped.push(format!(
                    "{} (target {to}/{name} exists; merge by hand)",
                    rel(&p)
                ));
                continue;
            }
            crate::write::file(
                &target,
                normalize(
                    &text,
                    kind,
                    &label,
                    date.as_deref().filter(|_| *dated),
                    None,
                    &today,
                    slug,
                ),
            )?;
            r.moved.push((rel(&p), format!("{to}/{name}")));
        }
    }
    migrate_locations(&world.join("locations"), &dir, &today, slug, &rel, &mut r)?;
    if let Ok(text) = fs::read_to_string(world.join("meta/conflicts.md")) {
        for block in format!("\n{text}").split("\n## ").skip(1) {
            let (title, body) = block.split_once('\n').unwrap_or((block, ""));
            let name = slugify(title);
            if name.is_empty() {
                r.skipped.push(format!(
                    "references/world/meta/conflicts.md ## {} (heading has no slug characters; move by hand)",
                    title.trim()
                ));
                continue;
            }
            let file = format!("life/conflicts/{name}.md");
            if crate::write::exists(&dir.join(&file)) {
                r.skipped.push(format!(
                    "references/world/meta/conflicts.md ## {} (target {file} exists; merge by hand)",
                    title.trim()
                ));
                continue;
            }
            let doc = format!(
                "# {}\n\n## Account A\n\n{}\n\n## Account B\n\n## Resolution\n",
                title.trim(),
                body.trim()
            );
            crate::write::file(
                &dir.join(&file),
                normalize(&doc, "conflict", &name, None, None, &today, slug),
            )?;
            r.moved
                .push(("references/world/meta/conflicts.md".into(), file));
        }
    }
    if world.join("meta/changelog.md").is_file() {
        r.skipped
            .push("references/world/meta/changelog.md (Git history is the change log)".into());
    }
    let from = src.join("sources");
    for e in walkdir::WalkDir::new(&from)
        .sort_by_file_name()
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
    {
        let name = e.path().strip_prefix(&from).expect("under sources");
        let name = name.to_string_lossy().replace('\\', "/");
        let target = root.join("sources").join(slug).join(&name);
        if crate::write::exists(&target) {
            r.skipped.push(format!(
                "{} (target <root>/sources/{slug}/{name} exists; merge by hand)",
                rel(e.path())
            ));
            continue;
        }
        crate::write::file(&target, fs::read(e.path())?)?;
        r.moved
            .push((rel(e.path()), format!("<root>/sources/{slug}/{name}")));
    }
    let known: Vec<String> = r.moved.iter().map(|(f, _)| f.clone()).collect();
    for e in walkdir::WalkDir::new(refs)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file() && !matches!(stem_of(e.path()).as_str(), "index" | "_index")
        })
    {
        let p = rel(e.path());
        if !known.contains(&p) && !r.skipped.iter().any(|s| s.starts_with(&p)) {
            r.skipped.push(format!("{p} (no owo home; move by hand)"));
        }
    }

    let id = crate::identity::Identity::load(root, slug)?;
    crate::index::write(&id)?;
    crate::stamp::run(&id, false)?;
    r.checklist = vec![
        "Move MEMORY \"Moments\" into event links under \"Defining moments\".".into(),
        "Move open promises and plans into life/threads/ (`owo add thread`).".into(),
        "Replace repeated accounts and quotes in logs with event links (`owo lint --rule dupes`).".into(),
        "Move exact personal identifiers into each character's `## Identifiers` (`owo lint --rule identifiers`).".into(),
        "Name account kinds in IDENTITY.md and keep handles in identity.json accounts.".into(),
        "Fill empty summaries, and link each room from its location overview.".into(),
        "Run `owo stamp && owo index && owo lint`, then the first review (`owo review`, `owo review done`).".into(),
    ];
    Ok(r)
}
