//! `owo migrate v1 <path> --out <root>/<slug>`: the mechanical part of moving a v1
//! identity (a `references/` tree) into the owo layout. Prints a checklist for the rest.

use crate::markdown::{parse_frontmatter, Fields};
use crate::slug::slugify;
use anyhow::{bail, Context, Result};
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
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
        .ok()
        .map(|_| d.to_string())
}

fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|s| s.trim().to_string())
}

fn frontmatter(text: &str) -> Fields {
    crate::markdown::split(text)
        .frontmatter
        .and_then(|fm| parse_frontmatter(fm).ok())
        .unwrap_or_default()
}

/// The title a migrated file gets: frontmatter `title`, else the first `# ` heading, else the stem.
fn title_of(text: &str, stem: &str) -> String {
    frontmatter(text)
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| first_heading(crate::markdown::split(text).body))
        .unwrap_or_else(|| stem.replace('-', " "))
}

/// Splits a v1 body into its `## Sources` citations, the links among them, and the body
/// without that section. `sources/<file>` paths become `<slug>/<file>`. A bullet with a link
/// counts as both: its links (with `#section`) for a lesson's `evidence`, its text as a citation.
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

/// Appends `items` to the list at `key`, skipping ones it has. A missing key becomes a list,
/// and a single string value becomes its first item.
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

/// Adds the owo frontmatter fields a v1 file lacks; keeps every existing field.
/// `parent` is the overview title for a location detail.
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
    // Body citations join any list the frontmatter already has. A lesson's linked bullets are its
    // evidence, not citations.
    let cites: Vec<String> = match kind {
        "lesson" => {
            merge(&mut map, "evidence", links);
            cites.into_iter().filter(|c| !c.contains("[[")).collect()
        }
        _ => cites,
    };
    merge(&mut map, "sources", cites);
    map.insert("type".to_string(), Value::from(kind));
    // v1 status values such as "established" mean canon.
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

/// v1 `world/locations/`: a folder `<x>/` becomes `life/locations/<x>/` with its overview
/// (`overview.md`, `index.md`, `README.md`, `<x>.md` inside, or `<x>.md` beside it) and its
/// other files as location details. A folder with no such file gets a stub overview.
/// A loose `<x>.md` becomes `life/locations/<x>/overview.md`.
fn migrate_locations(
    locs: &Path,
    dir: &Path,
    today: &str,
    slug: &str,
    rel: &dyn Fn(&Path) -> String,
    r: &mut Report,
) -> Result<()> {
    let mut write = |from: &Path, text: &str, to: String, kind: &str, parent: Option<&str>| {
        let target = dir.join(&to);
        if target.exists() {
            r.skipped
                .push(format!("{} (target {to} exists; merge by hand)", rel(from)));
            return anyhow::Ok(());
        }
        fs::create_dir_all(target.parent().expect("inside life/locations"))?;
        let stem = from.file_stem().map(|s| s.to_string_lossy().into_owned());
        let stem = stem.unwrap_or_default();
        fs::write(
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
        // A v1 index that is not the overview is dropped, as everywhere else.
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
        ("cultures", "life/cultures", "culture", false),
        ("factions", "life/factions", "faction", false),
        ("eras", "life/eras", "era", false),
        ("history/eras", "life/eras", "era", false),
        ("threads", "life/threads", "thread", false),
    ];
    for (from, to, kind, dated) in groups {
        // v1 index files are generated; owo writes its own.
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
            let date = date.or_else(|| {
                let fm = crate::markdown::split(&text).frontmatter?;
                let m = parse_frontmatter(fm).ok()?;
                m.get("date").and_then(Value::as_str).map(str::to_string)
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
            if target.exists() {
                r.skipped.push(format!(
                    "{} (target {to}/{name} exists; merge by hand)",
                    rel(&p)
                ));
                continue;
            }
            fs::write(
                &target,
                normalize(
                    &text,
                    kind,
                    &label,
                    date.as_deref().filter(|_| *dated),
                    None,
                    &today,
                    &slug,
                ),
            )?;
            r.moved.push((rel(&p), format!("{to}/{name}")));
        }
    }
    migrate_locations(&world.join("locations"), &dir, &today, &slug, &rel, &mut r)?;
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
            if dir.join(&file).exists() {
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
            fs::write(
                dir.join(&file),
                normalize(&doc, "conflict", &name, None, None, &today, &slug),
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
        let target = root.join("sources").join(&slug).join(&name);
        if target.exists() {
            r.skipped.push(format!(
                "{} (target <root>/sources/{slug}/{name} exists; merge by hand)",
                rel(e.path())
            ));
            continue;
        }
        fs::create_dir_all(target.parent().expect("has parent"))?;
        fs::copy(e.path(), &target)?;
        r.moved
            .push((rel(e.path()), format!("<root>/sources/{slug}/{name}")));
    }
    let known: Vec<String> = r.moved.iter().map(|(f, _)| f.clone()).collect();
    for e in walkdir::WalkDir::new(&refs)
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
        "Fill empty summaries, and link each room from its location overview.".into(),
        "Run `owo stamp && owo index && owo lint`, then the first review (`owo review`, `owo review done`).".into(),
    ];
    Ok(r)
}
