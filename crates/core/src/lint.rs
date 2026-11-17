use crate::identity::{life_categories, Entry, Identity, Kind, PERSONA_FILES, SESSIONS};
use crate::markdown;
use crate::output::{Diagnostic, Level};
use crate::slug::is_slug;
use crate::{index, state};
use chrono::NaiveDate;
use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;

pub const BUDGETS: &[(&str, usize)] = &[
    ("IDENTITY.md", 800),
    ("SOUL.md", 800),
    ("VOICE.md", 1000),
    ("LOOK.md", 600),
    ("MEMORY.md", 1500),
];

const DUPE_WORDS: usize = 12;
const STATUSES: &[&str] = &["canon", "disputed", "superseded"];

type Check = fn(&Identity, &mut Vec<Diagnostic>);

const CHECKS: &[(&str, Check)] = &[
    ("tree", tree),
    ("schema", schema),
    ("persona-plain", persona_plain),
    ("links", links),
    ("unique", unique),
    ("identifiers", identifiers),
    ("journal", journal),
    ("supersede", supersede),
    ("generated", generated),
    ("memory-reflection", memory_reflection),
    ("state", state_rule),
    ("budget", budget),
    ("dupes", dupes),
    ("orphans", orphans),
    ("persona-evidence", persona_evidence),
    ("reflection-due", reflection_due),
    ("wiki", wiki_links),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule(usize);

impl Rule {
    pub fn all() -> impl Iterator<Item = Rule> {
        (0..CHECKS.len()).map(Rule)
    }
}

impl crate::Named for Rule {
    fn rows() -> impl Iterator<Item = (Rule, &'static str)> {
        CHECKS.iter().enumerate().map(|(i, c)| (Rule(i), c.0))
    }
}

pub fn run(id: &Identity, rules: &[Rule]) -> Vec<Diagnostic> {
    let mut out = vec![];
    for (i, (name, check)) in CHECKS.iter().enumerate() {
        if rules.is_empty() || rules.contains(&Rule(i)) {
            let start = out.len();
            check(id, &mut out);
            for d in &mut out[start..] {
                d.rule = name.to_string();
            }
        }
    }
    out.sort_by(|a, b| {
        (a.level, &a.path, a.line, &a.rule).cmp(&(b.level, &b.path, b.line, &b.rule))
    });
    out
}

fn err(path: &str, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Level::Error, path, msg)
}

fn warn(path: &str, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Level::Warning, path, msg)
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    state::parse_day(value.trim()).ok()
}

fn tree(id: &Identity, out: &mut Vec<Diagnostic>) {
    let dir = id.dir.clone();
    let mut required: Vec<String> = vec![
        "identity.json".into(),
        "persona/soul.json".into(),
        "persona/state.json".into(),
        "life/state.json".into(),
        "work/state.json".into(),
        "work/PROFILE.md".into(),
    ];
    required.extend(PERSONA_FILES.iter().map(|f| format!("persona/{f}")));
    for rel in &required {
        if !dir.join(rel).is_file() {
            out.push(err(rel, "required file is missing"));
        }
    }
    let mut dirs: Vec<String> = life_categories()
        .map(|(cat, _)| format!("life/{cat}"))
        .collect();
    dirs.push("work/projects".into());
    for rel in &dirs {
        if !dir.join(rel).is_dir() {
            out.push(err(rel, "required folder is missing"));
        }
    }
    let ignores_sessions = fs::read_to_string(dir.join(".gitignore"))
        .is_ok_and(|text| text.lines().any(|l| l.trim().trim_matches('/') == SESSIONS));
    if !ignores_sessions {
        out.push(warn(
            ".gitignore",
            "the line `sessions/` is missing; Git would track the session files",
        ));
    }

    match (&id.manifest, &id.manifest_error) {
        (_, Some(e)) => out.push(err("identity.json", format!("invalid JSON: {e}"))),
        (m, None) if !m.is_null() => {
            if m.get("schema").and_then(Json::as_str) != Some("owo/1") {
                out.push(err("identity.json", "schema must be \"owo/1\""));
            }
            if m.get("slug").and_then(Json::as_str) != Some(id.slug.as_str()) {
                out.push(err(
                    "identity.json",
                    format!("slug must equal the folder name {:?}", id.slug),
                ));
            }
        }
        _ => {}
    }
    if dir.join("persona/soul.json").is_file() {
        match id.read_json("persona/soul.json") {
            None => out.push(err("persona/soul.json", "invalid JSON")),
            Some(s) => {
                for key in ["specVersion", "name", "displayName", "version"] {
                    if s.get(key).and_then(Json::as_str).is_none_or(str::is_empty) {
                        out.push(err(
                            "persona/soul.json",
                            format!("SoulSpec field {key:?} is missing"),
                        ));
                    }
                }
            }
        }
    }

    let locations = dir.join("life/locations");
    if let Ok(items) = fs::read_dir(&locations) {
        for item in items.flatten() {
            let path = item.path();
            let name = item.file_name().to_string_lossy().to_string();
            if !path.is_dir() {
                continue;
            }
            let rel = format!("life/locations/{name}");
            if !path.join("overview.md").is_file() {
                out.push(err(&rel, "location folder needs overview.md"));
            }
            if !is_slug(&name) {
                out.push(err(&rel, "location folder name must be a slug"));
            }
            if let Ok(inner) = fs::read_dir(&path) {
                for sub in inner.flatten().filter(|s| s.path().is_dir()) {
                    out.push(err(
                        &format!("{rel}/{}", sub.file_name().to_string_lossy()),
                        "location folders sit one level deep",
                    ));
                }
            }
        }
    }
    for e in id.entries.iter().filter(|e| e.kind == Kind::Unknown) {
        out.push(err(&e.rel, "file sits outside the identity layout"));
    }
    for e in id.entries.iter().filter(|e| e.lossy) {
        out.push(err(
            &e.rel,
            "file is not valid UTF-8; owo reads the bad bytes as U+FFFD",
        ));
    }
}

fn schema(id: &Identity, out: &mut Vec<Diagnostic>) {
    for e in id.entries.iter().filter(|e| e.kind.has_frontmatter()) {
        check_entry_schema(e, out);
    }
    let mut logs: BTreeMap<NaiveDate, &str> = BTreeMap::new();
    for e in id.entries.iter().filter(|e| e.kind == Kind::DailyLog) {
        let Some(day) = e.field_str("date").as_deref().and_then(parse_date) else {
            continue;
        };
        match logs.get(&day) {
            Some(first) => out.push(err(
                &e.rel,
                format!("{day} has a second daily log; merge it into {first}"),
            )),
            None => {
                logs.insert(day, &e.rel);
            }
        }
    }
}

fn check_entry_schema(e: &Entry, out: &mut Vec<Diagnostic>) {
    let rel = e.rel.as_str();
    let expected = e.kind.type_name().expect("frontmatter kinds have a type");
    if let Some(msg) = &e.frontmatter_error {
        out.push(err(rel, format!("frontmatter does not parse: {msg}")));
        return;
    }
    if e.frontmatter.is_none() {
        let msg = if e.text.starts_with("---") {
            "frontmatter is not closed; end it with a `---` line"
        } else {
            "frontmatter is missing"
        };
        out.push(err(rel, msg));
        return;
    }
    match e.field_str("type") {
        Some(t) if t == expected => {}
        Some(t) => out.push(err(
            rel,
            format!("type is {t:?}; files here are {expected:?}"),
        )),
        None => out.push(err(rel, "type is missing")),
    }
    if e.field_str("title").is_none_or(|t| t.trim().is_empty()) {
        out.push(err(rel, "title is missing"));
    }
    for name in e.title.iter().chain(&e.aliases) {
        if name.contains(crate::markdown::WIKILINK_BREAKERS) {
            out.push(err(
                rel,
                format!("{name:?} holds # | [ or ], which break a [[wikilink]] to it"),
            ));
        }
    }
    match e.field_str("status") {
        Some(s) if STATUSES.contains(&s.as_str()) => {}
        Some(s) => out.push(err(
            rel,
            format!("status {s:?} must be one of canon, disputed, superseded"),
        )),
        None => out.push(err(rel, "status is missing")),
    }
    for key in ["created", "updated"] {
        match e.field_str(key) {
            Some(v) if parse_date(&v).is_some() => {}
            Some(v) => out.push(err(rel, format!("{key} {v:?} is not a YYYY-MM-DD date"))),
            None => out.push(err(rel, format!("{key} is missing"))),
        }
    }
    for key in ["aliases", "sources"] {
        match e.field(key) {
            None | Some(Json::Null) => {}
            Some(Json::Array(items)) if items.iter().all(|i| i.is_string()) => {}
            Some(_) => out.push(err(rel, format!("{key} must be a list of strings"))),
        }
    }
    let needs_summary = !matches!(e.kind, Kind::DailyLog | Kind::Journal | Kind::Writing);
    if needs_summary && e.field_str("summary").is_none_or(|s| s.trim().is_empty()) {
        out.push(warn(rel, "summary is empty; indexes and search use it"));
    }

    let file = rel
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches(".md");
    if e.kind.dated() {
        match e.field_str("date").as_deref().and_then(parse_date) {
            None => out.push(err(rel, "date is missing or not YYYY-MM-DD")),
            Some(d) => {
                let d = d.format("%Y-%m-%d").to_string();
                if !file.strip_prefix(&format!("{d}_")).is_some_and(is_slug) {
                    out.push(err(rel, format!("file name must be {d}_<slug>.md")));
                }
            }
        }
    } else {
        let stem = e.stem_key();
        if !is_slug(&stem) && !(e.kind == Kind::Location && file == "overview") {
            out.push(err(rel, format!("file name {stem:?} must be a slug")));
        }
    }
    let state_field =
        |key: &str, allowed: &[&str], out: &mut Vec<Diagnostic>| match e.field_str(key) {
            Some(v) if allowed.contains(&v.as_str()) => {}
            _ => out.push(err(
                rel,
                format!("{key} must be one of {}", allowed.join(", ")),
            )),
        };
    match e.kind {
        Kind::Event => {
            if e.field_str("date")
                .as_deref()
                .and_then(parse_date)
                .is_none()
            {
                out.push(err(rel, "event date is missing or not YYYY-MM-DD"));
            }
            if e.field_str("end_date")
                .is_some_and(|v| parse_date(&v).is_none())
            {
                out.push(err(rel, "end_date is not YYYY-MM-DD"));
            }
        }
        Kind::Thread => state_field("state", &["open", "closed"], out),
        Kind::Conflict => state_field("state", &["open", "resolved"], out),
        Kind::LocationDetail => {
            if e.field_str("parent").is_none_or(|p| p.trim().is_empty()) {
                out.push(err(rel, "parent is missing"));
            }
        }
        Kind::ProjectNote => {
            let folder = rel.split('/').nth(2).unwrap_or_default();
            if e.field_str("project").as_deref() != Some(folder) {
                out.push(err(rel, format!("project must be {folder:?}")));
            }
        }
        _ => {}
    }
}

fn persona_plain(id: &Identity, out: &mut Vec<Diagnostic>) {
    out.extend(
        id.entries
            .iter()
            .filter(|e| e.kind == Kind::Persona && e.text.starts_with("---"))
            .map(|e| {
                err(
                    &e.rel,
                    "persona files are plain Markdown; stamps live in persona/state.json",
                )
            }),
    );
}

fn links(id: &Identity, out: &mut Vec<Diagnostic>) {
    for e in id.entries.iter().filter(|e| !e.is_generated()) {
        for (line, link) in crate::find::file_wikilinks(e) {
            let hits = id.resolve_wikilink(&link.target);
            let d = match hits.len() {
                0 => err(&e.rel, format!("[[{}]] does not resolve", link.target)),
                1 => {
                    let target = &id.entries[hits[0]];
                    let Some(s) = link.section.as_deref().filter(|s| !s.is_empty()) else {
                        continue;
                    };
                    if markdown::section(target.body(), s).is_some() {
                        continue;
                    }
                    warn(
                        &e.rel,
                        format!(
                            "[[{}#{s}]]: {} has no section {s:?}",
                            link.target, target.rel
                        ),
                    )
                }
                _ => warn(
                    &e.rel,
                    format!("[[{}]] matches {} entries", link.target, hits.len()),
                ),
            };
            out.push(d.at(line));
        }
    }
}

fn unique(id: &Identity, out: &mut Vec<Diagnostic>) {
    let mut seen: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for e in id
        .entries
        .iter()
        .filter(|e| !e.is_generated() && e.kind != Kind::Example)
    {
        let mut names: BTreeSet<String> =
            e.aliases.iter().map(|a| a.trim().to_lowercase()).collect();
        if let Some(t) = &e.title {
            names.insert(t.trim().to_lowercase());
        }
        for n in names {
            seen.entry(n).or_default().push(&e.rel);
        }
    }
    for (name, paths) in seen.into_iter().filter(|(_, p)| p.len() > 1) {
        for p in &paths {
            out.push(err(
                p,
                format!(
                    "title or alias {name:?} is also used by {}",
                    paths
                        .iter()
                        .filter(|o| *o != p)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }
}

fn identifier_values(id: &Identity) -> Vec<(String, String)> {
    let mut out = vec![];
    for e in id.entries.iter().filter(|e| !e.is_generated()) {
        if let Some((_, lines)) = markdown::section(e.body(), "Identifiers") {
            for (_, l) in lines {
                let Some(item) = l.trim().strip_prefix("- ") else {
                    continue;
                };
                let value = item.split_once(':').map(|(_, v)| v).unwrap_or(item).trim();
                if value.chars().count() >= 3 {
                    out.push((value.to_string(), e.rel.clone()));
                }
            }
        }
    }
    if let Some(accounts) = id.manifest["accounts"].as_array() {
        for a in accounts {
            if let Some(h) = a
                .get("handle")
                .and_then(Json::as_str)
                .filter(|h| h.chars().count() >= 3)
            {
                out.push((h.to_string(), "identity.json".into()));
            }
        }
    }
    out
}

fn identifiers(id: &Identity, out: &mut Vec<Diagnostic>) {
    let values = identifier_values(id);
    for e in &id.entries {
        let split = markdown::split(&e.text);
        let home_lines: Vec<usize> = markdown::section(split.body, "Identifiers")
            .map(|(_, lines)| lines.iter().map(|(i, _)| i + split.body_line).collect())
            .unwrap_or_default();
        for (value, home) in &values {
            let own = &e.rel == home;
            if own && home == "identity.json" {
                continue;
            }
            for (i, line) in e.text.lines().enumerate() {
                if line.contains(value.as_str()) && !(own && home_lines.contains(&(i + 1))) {
                    let msg = if own {
                        format!("identifier {value:?} belongs only under ## Identifiers")
                    } else {
                        format!("identifier {value:?} belongs only in {home}")
                    };
                    out.push(err(&e.rel, msg).at(i + 1));
                }
            }
        }
    }
}

fn journal(id: &Identity, out: &mut Vec<Diagnostic>) {
    for e in id
        .entries
        .iter()
        .filter(|e| e.kind != Kind::Journal && !e.is_generated())
    {
        for (line, link) in crate::find::raw_wikilinks(e) {
            let hits = id.resolve_wikilink(&link.target);
            if hits.iter().any(|&i| id.entries[i].kind == Kind::Journal) {
                out.push(err(&e.rel, format!("[[{}]] links into life/journal/; journal wikilinks stay between journal entries", link.target)).at(line));
            }
        }
    }
}

fn wikilink_hits(id: &Identity, value: &str) -> Vec<usize> {
    markdown::wikilinks(value)
        .iter()
        .filter_map(|l| match id.resolve_wikilink(&l.target)[..] {
            [one] => Some(one),
            _ => None,
        })
        .collect()
}

fn supersede(id: &Identity, out: &mut Vec<Diagnostic>) {
    let open_conflict_targets: BTreeSet<usize> = id
        .entries
        .iter()
        .filter(|e| e.kind == Kind::Conflict && e.field_str("state").as_deref() == Some("open"))
        .flat_map(|e| {
            crate::find::raw_wikilinks(e)
                .into_iter()
                .flat_map(|(_, l)| id.resolve_wikilink(&l.target))
        })
        .collect();
    for (idx, e) in id.entries.iter().enumerate() {
        match e.field_str("status").as_deref() {
            Some("superseded") => {
                let target = e.field_str("superseded_by").unwrap_or_default();
                if wikilink_hits(id, &target).is_empty() {
                    out.push(err(
                        &e.rel,
                        "superseded entries need superseded_by: \"[[Replacement]]\" that resolves",
                    ));
                }
            }
            Some("disputed") if !open_conflict_targets.contains(&idx) => {
                out.push(err(
                    &e.rel,
                    "disputed entries need an open conflict in life/conflicts/ that links here",
                ));
            }
            _ => {}
        }
    }
}

fn generated(id: &Identity, out: &mut Vec<Diagnostic>) {
    for rel in index::stale(id) {
        out.push(err(&rel, "generated file is out of date; run `owo index`"));
    }
}

fn memory_reflection(id: &Identity, out: &mut Vec<Diagnostic>) {
    if state::doc(id, "persona").is_null() || !state::reflection(id).memory_changed {
        return;
    }
    out.push(warn(
        "persona/MEMORY.md",
        "MEMORY.md changed outside a reflection; a reflection compiles MEMORY (`owo reflect`)",
    ));
}

fn state_rule(id: &Identity, out: &mut Vec<Diagnostic>) {
    let report = match crate::stamp::run(id, true) {
        Ok(r) => r,
        Err(e) => {
            let path = state::SCOPES
                .iter()
                .map(|s| format!("{s}/state.json"))
                .find(|p| id.read_object(p).is_err())
                .unwrap_or_default();
            return out.push(warn(&path, format!("{e:#}")));
        }
    };
    for rel in &report.added {
        let scope = state::scope_of(rel).map_or("", |(s, _)| s);
        out.push(warn(rel, format!("not stamped in {scope}/state.json")));
    }
    for rel in &report.changed {
        out.push(warn(rel, "changed since the last stamp"));
    }
    for gone in &report.removed {
        let (scope, rel) = gone.split_once('/').expect("scope/rel");
        out.push(warn(
            &format!("{scope}/state.json"),
            format!("stamps {rel}, which is gone"),
        ));
    }
}

fn budget(id: &Identity, out: &mut Vec<Diagnostic>) {
    for (file, default) in BUDGETS {
        let Some(e) = id.entry(&format!("persona/{file}")) else {
            continue;
        };
        let limit = id.budget_override(file).unwrap_or(*default);
        let words = markdown::word_count(&e.text);
        if words > limit {
            out.push(warn(&e.rel, format!("{words} words, soft budget {limit}")));
        }
    }
}

fn shingles(body: &str) -> BTreeSet<String> {
    let words: Vec<String> = body
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|ch| ch.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect();
    words.windows(DUPE_WORDS).map(|w| w.join(" ")).collect()
}

fn dupes(id: &Identity, out: &mut Vec<Diagnostic>) {
    let authored: Vec<&Entry> = id
        .entries
        .iter()
        .filter(|e| !e.is_generated() && e.kind != Kind::Example)
        .collect();
    let mut owners: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in authored.iter().enumerate() {
        for s in shingles(e.body()) {
            owners.entry(s).or_default().push(i);
        }
    }
    let mut pairs: BTreeMap<(usize, usize), (usize, &str)> = BTreeMap::new();
    for (s, files) in owners.iter().filter(|(_, f)| f.len() > 1) {
        for (a, x) in files.iter().enumerate() {
            for y in &files[a + 1..] {
                let pair = pairs.entry((*x, *y)).or_insert((0, s));
                pair.0 += 1;
                pair.1 = pair.1.min(s);
            }
        }
    }
    for ((a, b), (n, passage)) in pairs {
        let windows = if n == 1 { "window" } else { "windows" };
        out.push(warn(&authored[a].rel,
            format!(
                "shares text with {} (\"{passage}\", {n} overlapping {DUPE_WORDS}-word {windows}); keep it in one home and link",
                authored[b].rel
            ),
        ));
    }
}

fn orphans(id: &Identity, out: &mut Vec<Diagnostic>) {
    out.extend(
        crate::find::orphans(id)
            .into_iter()
            .map(|i| warn(&id.entries[i].rel, "zero inbound wikilinks")),
    );
}

fn persona_evidence(id: &Identity, out: &mut Vec<Diagnostic>) {
    for (n, record) in state::changelog(id).iter().enumerate() {
        let evidence: std::collections::BTreeSet<usize> = record
            .get("evidence")
            .and_then(Json::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Json::as_str)
                    .flat_map(|v| wikilink_hits(id, v))
                    .collect()
            })
            .unwrap_or_default();
        let defining = evidence.iter().any(|&i| {
            let e = &id.entries[i];
            e.kind == Kind::Event && matches!(e.field("defining"), Some(Json::Bool(true)))
        });
        if evidence.len() < 2 && !defining {
            out.push(warn(
                "persona/state.json",
                format!("changelog[{n}] needs two evidence wikilinks or one defining event"),
            ));
        }
    }
}

fn reflection_due(id: &Identity, out: &mut Vec<Diagnostic>) {
    let r = state::reflection(id);
    let msg = match (&r.last_reflected, r.days_since) {
        (None, _) => Some(
            "never reflected; run `owo reflect` when there is something to compile".to_string(),
        ),
        (Some(_), None) => {
            Some("reflection.last_reflected is not an RFC 3339 timestamp".to_string())
        }
        (Some(_), Some(days)) => r.due.then(|| {
            format!(
                "reflection is due: {days} days since the last reflection (every {})",
                r.every_days
            )
        }),
    };
    if let Some(m) = msg {
        out.push(Diagnostic::new(Level::Info, "persona/state.json", m));
    }
}

fn wiki_links(id: &Identity, out: &mut Vec<Diagnostic>) {
    for e in id.entries.iter().filter(|e| !e.is_generated()) {
        for (line, link) in crate::find::raw_wikilinks(e) {
            let full = link
                .section
                .as_ref()
                .map_or(link.target.clone(), |s| format!("{}#{s}", link.target));
            let d = match crate::wiki::link(id, &full) {
                Some(Err(msg)) => err(&e.rel, msg),
                Some(Ok(k)) if k.root_exists && !k.file_exists => warn(
                    &e.rel,
                    format!(
                        "[[{}]]: {} not found",
                        link.target,
                        k.file.unwrap_or_default()
                    ),
                ),
                Some(Ok(k)) if k.root_exists && !k.section_exists => warn(
                    &e.rel,
                    format!(
                        "[[{}]]: no section {:?}",
                        link.target,
                        k.section.unwrap_or_default()
                    ),
                ),
                _ => continue,
            };
            out.push(d.at(line));
        }
    }
}
