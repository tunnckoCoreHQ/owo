//! `owo lint`: validates one identity against the design rules.

use crate::identity::{Entry, Identity, Kind, LIFE_CATEGORIES, SOUL_FILES};
use crate::markdown;
use crate::output::{Diagnostic, Level};
use crate::slug::is_slug;
use crate::{index, state};
use chrono::NaiveDate;
use serde_json::Value as Json;
use serde_json::Value as Yaml;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;

/// Soft word budgets for soul files.
pub const BUDGETS: &[(&str, usize)] = &[
    ("IDENTITY.md", 800),
    ("SOUL.md", 800),
    ("VOICE.md", 1000),
    ("LOOK.md", 600),
    ("MEMORY.md", 1500),
];

const DUPE_WORDS: usize = 12;
const STATUSES: &[&str] = &["canon", "disputed", "superseded"];

pub struct Options {
    pub rules: Vec<String>,
}

type Check = fn(&mut Ctx);

struct Ctx<'a> {
    id: &'a Identity,
    keys: BTreeMap<String, Vec<usize>>,
    out: Vec<Diagnostic>,
}

impl Ctx<'_> {
    fn push(&mut self, d: Diagnostic) {
        self.out.push(d);
    }
}

/// Every rule, in run order.
const CHECKS: &[(&str, Check)] = &[
    ("tree", tree),
    ("schema", schema),
    ("soul-plain", soul_plain),
    ("links", links),
    ("unique", unique),
    ("identifiers", identifiers),
    ("journal", journal),
    ("supersede", supersede),
    ("generated", generated),
    ("memory-review", memory_review),
    ("state", state_rule),
    ("budget", budget),
    ("dupes", dupes),
    ("orphans", orphans),
    ("soul-evidence", soul_evidence),
    ("review-due", review_due),
    ("kb", kb_links),
];

/// Rule names for `--rule`.
pub fn rules() -> impl Iterator<Item = &'static str> {
    CHECKS.iter().map(|(name, _)| *name)
}

pub fn run(id: &Identity, opts: &Options) -> Vec<Diagnostic> {
    let mut ctx = Ctx {
        id,
        keys: id.link_keys(),
        out: vec![],
    };
    let enabled = |rule: &str| opts.rules.is_empty() || opts.rules.iter().any(|r| r == rule);
    for (name, check) in CHECKS {
        if enabled(name) {
            check(&mut ctx);
        }
    }
    let mut out = ctx.out;
    out.sort_by(|a, b| {
        (a.level, &a.path, a.line, &a.rule).cmp(&(b.level, &b.path, b.line, &b.rule))
    });
    out
}

fn err(rule: &str, path: &str, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::new(rule, Level::Error, path, msg)
}

fn warn(rule: &str, path: &str, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::new(rule, Level::Warning, path, msg)
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    state::parse_day(value.trim()).ok()
}

fn tree(c: &mut Ctx) {
    let dir = c.id.dir.clone();
    let mut required: Vec<String> = vec![
        "identity.json".into(),
        "soul/soul.json".into(),
        "soul/state.json".into(),
        "life/state.json".into(),
        "work/state.json".into(),
        "work/README.md".into(),
    ];
    required.extend(SOUL_FILES.iter().map(|f| format!("soul/{f}")));
    for rel in &required {
        if !dir.join(rel).is_file() {
            c.push(err("tree", rel, "required file is missing"));
        }
    }
    let mut dirs: Vec<String> = LIFE_CATEGORIES
        .iter()
        .map(|(cat, _)| format!("life/{cat}"))
        .collect();
    dirs.push("work/projects".into());
    for rel in &dirs {
        if !dir.join(rel).is_dir() {
            c.push(err("tree", rel, "required folder is missing"));
        }
    }

    match (&c.id.manifest, &c.id.manifest_error) {
        (_, Some(e)) => c.push(err("tree", "identity.json", format!("invalid JSON: {e}"))),
        (Some(m), None) => {
            if m.get("schema").and_then(Json::as_str) != Some("owo/1") {
                c.push(err("tree", "identity.json", "schema must be \"owo/1\""));
            }
            if m.get("slug").and_then(Json::as_str) != Some(c.id.slug.as_str()) {
                c.push(err(
                    "tree",
                    "identity.json",
                    format!("slug must equal the folder name {:?}", c.id.slug),
                ));
            }
        }
        _ => {}
    }
    if dir.join("soul/soul.json").is_file() {
        match c.id.read_json("soul/soul.json") {
            None => c.push(err("tree", "soul/soul.json", "invalid JSON")),
            Some(s) => {
                for key in ["specVersion", "name", "displayName", "version"] {
                    if s.get(key).and_then(Json::as_str).is_none_or(str::is_empty) {
                        c.push(err(
                            "tree",
                            "soul/soul.json",
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
                c.push(err("tree", &rel, "location folder needs overview.md"));
            }
            if !is_slug(&name) {
                c.push(err("tree", &rel, "location folder name must be a slug"));
            }
            if let Ok(inner) = fs::read_dir(&path) {
                for sub in inner.flatten().filter(|s| s.path().is_dir()) {
                    c.push(err(
                        "tree",
                        &format!("{rel}/{}", sub.file_name().to_string_lossy()),
                        "location folders sit one level deep",
                    ));
                }
            }
        }
    }
    let unknown: Vec<String> =
        c.id.entries
            .iter()
            .filter(|e| e.kind == Kind::Unknown)
            .map(|e| e.rel.clone())
            .collect();
    for rel in unknown {
        c.push(err("tree", &rel, "file sits outside the identity layout"));
    }
    let lossy: Vec<String> =
        c.id.entries
            .iter()
            .filter(|e| e.lossy)
            .map(|e| e.rel.clone())
            .collect();
    for rel in lossy {
        c.push(err(
            "tree",
            &rel,
            "file is not valid UTF-8; owo reads the bad bytes as U+FFFD",
        ));
    }
}

fn schema(c: &mut Ctx) {
    let mut found = vec![];
    for e in c.id.entries.iter().filter(|e| e.kind.has_frontmatter()) {
        check_entry_schema(e, &mut found);
    }
    c.out.extend(found);
}

fn check_entry_schema(e: &Entry, out: &mut Vec<Diagnostic>) {
    let rel = e.rel.as_str();
    let expected = e.kind.type_name().expect("frontmatter kinds have a type");
    if let Some(msg) = &e.frontmatter_error {
        out.push(err(
            "schema",
            rel,
            format!("frontmatter does not parse: {msg}"),
        ));
        return;
    }
    if e.frontmatter.is_none() {
        let msg = if e.text.starts_with("---") {
            "frontmatter is not closed; end it with a `---` line"
        } else {
            "frontmatter is missing"
        };
        out.push(err("schema", rel, msg));
        return;
    }
    match e.field_str("type") {
        Some(t) if t == expected => {}
        Some(t) => out.push(err(
            "schema",
            rel,
            format!("type is {t:?}; files here are {expected:?}"),
        )),
        None => out.push(err("schema", rel, "type is missing")),
    }
    if e.field_str("title").is_none_or(|t| t.trim().is_empty()) {
        out.push(err("schema", rel, "title is missing"));
    }
    match e.field_str("status") {
        Some(s) if STATUSES.contains(&s.as_str()) => {}
        Some(s) => out.push(err(
            "schema",
            rel,
            format!("status {s:?} must be one of canon, disputed, superseded"),
        )),
        None => out.push(err("schema", rel, "status is missing")),
    }
    for key in ["created", "updated"] {
        match e.field_str(key) {
            Some(v) if parse_date(&v).is_some() => {}
            Some(v) => out.push(err(
                "schema",
                rel,
                format!("{key} {v:?} is not a YYYY-MM-DD date"),
            )),
            None => out.push(err("schema", rel, format!("{key} is missing"))),
        }
    }
    for key in ["aliases", "sources"] {
        match e.field(key) {
            None | Some(Yaml::Null) => {}
            Some(Yaml::Array(items)) if items.iter().all(|i| i.is_string()) => {}
            Some(_) => out.push(err(
                "schema",
                rel,
                format!("{key} must be a list of strings"),
            )),
        }
    }
    let needs_summary = !matches!(e.kind, Kind::Log | Kind::Journal | Kind::Writing);
    if needs_summary && e.field_str("summary").is_none_or(|s| s.trim().is_empty()) {
        out.push(warn(
            "schema",
            rel,
            "summary is empty; indexes and search use it",
        ));
    }

    let file = rel
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches(".md");
    match e.kind.dated() {
        Some(with_slug) => {
            let date = e.field_str("date");
            match date.as_deref().and_then(parse_date) {
                None => out.push(err("schema", rel, "date is missing or not YYYY-MM-DD")),
                Some(d) => {
                    let d = d.format("%Y-%m-%d").to_string();
                    let ok = if with_slug {
                        file.strip_prefix(&format!("{d}_")).is_some_and(is_slug)
                    } else {
                        file == d
                    };
                    if !ok {
                        let want = if with_slug {
                            format!("{d}_<slug>.md")
                        } else {
                            format!("{d}.md")
                        };
                        out.push(err("schema", rel, format!("file name must be {want}")));
                    }
                }
            }
        }
        None => {
            let stem = e.stem_key();
            if e.kind != Kind::Project
                && !is_slug(&stem)
                && !(e.kind == Kind::Location && file == "overview")
            {
                out.push(err(
                    "schema",
                    rel,
                    format!("file name {stem:?} must be a slug"),
                ));
            }
        }
    }
    let state_field =
        |key: &str, allowed: &[&str], out: &mut Vec<Diagnostic>| match e.field_str(key) {
            Some(v) if allowed.contains(&v.as_str()) => {}
            _ => out.push(err(
                "schema",
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
                out.push(err(
                    "schema",
                    rel,
                    "event date is missing or not YYYY-MM-DD",
                ));
            }
            if let Some(v) = e.field_str("end_date") {
                if parse_date(&v).is_none() {
                    out.push(err("schema", rel, "end_date is not YYYY-MM-DD"));
                }
            }
        }
        Kind::Thread => state_field("state", &["open", "closed"], out),
        Kind::Conflict => state_field("state", &["open", "resolved"], out),
        Kind::LocationDetail => {
            if e.field_str("parent").is_none_or(|p| p.trim().is_empty()) {
                out.push(err("schema", rel, "parent is missing"));
            }
        }
        Kind::Note => {
            let folder = rel.split('/').nth(2).unwrap_or_default();
            if e.field_str("project").as_deref() != Some(folder) {
                out.push(err("schema", rel, format!("project must be {folder:?}")));
            }
        }
        _ => {}
    }
}

fn soul_plain(c: &mut Ctx) {
    let bad: Vec<String> =
        c.id.entries
            .iter()
            .filter(|e| e.kind == Kind::Soul && e.text.starts_with("---"))
            .map(|e| e.rel.clone())
            .collect();
    for rel in bad {
        c.push(err(
            "soul-plain",
            &rel,
            "soul files are plain Markdown; stamps live in soul/state.json",
        ));
    }
}

fn links(c: &mut Ctx) {
    let mut found = vec![];
    for e in c.id.entries.iter().filter(|e| !e.is_generated()) {
        for (line, link) in crate::find::file_links(e) {
            let hits = c.id.resolve_link(&c.keys, &link.target);
            match hits.len() {
                0 => found.push(
                    err(
                        "links",
                        &e.rel,
                        format!("[[{}]] does not resolve", link.target),
                    )
                    .at(line),
                ),
                1 => {
                    let target = &c.id.entries[hits[0]];
                    if let Some(s) = link.section.as_deref().filter(|s| !s.is_empty()) {
                        if markdown::section(target.body(), s).is_none() {
                            found.push(
                                warn(
                                    "links",
                                    &e.rel,
                                    format!(
                                        "[[{}#{s}]]: {} has no section {s:?}",
                                        link.target, target.rel
                                    ),
                                )
                                .at(line),
                            );
                        }
                    }
                }
                _ => found.push(
                    warn(
                        "links",
                        &e.rel,
                        format!("[[{}]] matches {} entries", link.target, hits.len()),
                    )
                    .at(line),
                ),
            }
        }
    }
    c.out.extend(found);
}

fn unique(c: &mut Ctx) {
    let mut seen: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for e in
        c.id.entries
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
    let mut found = vec![];
    for (name, paths) in seen.into_iter().filter(|(_, p)| p.len() > 1) {
        for p in &paths {
            found.push(err(
                "unique",
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
    c.out.extend(found);
}

/// Identifier values and their home file.
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
    if let Some(accounts) = id
        .manifest
        .as_ref()
        .and_then(|m| m.get("accounts"))
        .and_then(Json::as_array)
    {
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

fn identifiers(c: &mut Ctx) {
    let values = identifier_values(c.id);
    let mut found = vec![];
    for e in &c.id.entries {
        // In the home file, only the `## Identifiers` lines may hold the values.
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
                    found.push(err("identifiers", &e.rel, msg).at(i + 1));
                }
            }
        }
    }
    c.out.extend(found);
}

fn journal(c: &mut Ctx) {
    let mut found = vec![];
    for e in
        c.id.entries
            .iter()
            .filter(|e| e.kind != Kind::Journal && !e.is_generated())
    {
        for (line, link) in crate::find::raw_links(e) {
            let hits = c.id.resolve_link(&c.keys, &link.target);
            if hits.iter().any(|&i| c.id.entries[i].kind == Kind::Journal) {
                found.push(err("journal", &e.rel, format!("[[{}]] links into life/journal/; journal links stay between journal entries", link.target)).at(line));
            }
        }
    }
    c.out.extend(found);
}

fn link_hits(c: &Ctx, value: &str) -> Vec<usize> {
    markdown::links(value)
        .iter()
        .flat_map(|l| c.id.resolve_link(&c.keys, &l.target))
        .collect()
}

fn supersede(c: &mut Ctx) {
    let mut found = vec![];
    let open_conflict_targets: BTreeSet<usize> =
        c.id.entries
            .iter()
            .filter(|e| e.kind == Kind::Conflict && e.field_str("state").as_deref() == Some("open"))
            .flat_map(|e| {
                crate::find::raw_links(e)
                    .into_iter()
                    .flat_map(|(_, l)| c.id.resolve_link(&c.keys, &l.target))
            })
            .collect();
    for (idx, e) in c.id.entries.iter().enumerate() {
        match e.field_str("status").as_deref() {
            Some("superseded") => {
                let target = e.field_str("superseded_by").unwrap_or_default();
                if link_hits(c, &target).is_empty() {
                    found.push(err(
                        "supersede",
                        &e.rel,
                        "superseded entries need superseded_by: \"[[Replacement]]\" that resolves",
                    ));
                }
            }
            Some("disputed") if !open_conflict_targets.contains(&idx) => {
                found.push(err(
                    "supersede",
                    &e.rel,
                    "disputed entries need an open conflict in life/conflicts/ that links here",
                ));
            }
            _ => {}
        }
    }
    c.out.extend(found);
}

fn generated(c: &mut Ctx) {
    for rel in index::stale(c.id) {
        c.push(err(
            "generated",
            &rel,
            "generated file is out of date; run `owo index`",
        ));
    }
}

fn memory_review(c: &mut Ctx) {
    if state::doc(c.id, "soul").is_null() || !state::review(c.id).memory_changed {
        return;
    }
    c.push(warn(
        "memory-review",
        "soul/MEMORY.md",
        "MEMORY.md changed outside a review; review compiles MEMORY (`owo review`)",
    ));
}

/// The drift `owo stamp --check` reports, one warning per file.
fn state_rule(c: &mut Ctx) {
    let report = match crate::stamp::run(c.id, true) {
        Ok(r) => r,
        Err(e) => {
            // Only a state file that does not parse stops the check.
            let path = state::SCOPES
                .iter()
                .map(|s| format!("{s}/state.json"))
                .find(|p| c.id.read_object(p).is_err())
                .unwrap_or_default();
            return c.push(warn("state", &path, format!("{e:#}")));
        }
    };
    for rel in &report.added {
        let scope = state::scope_of(rel).map_or("", |(s, _)| s);
        c.push(warn(
            "state",
            rel,
            format!("not stamped in {scope}/state.json"),
        ));
    }
    for rel in &report.changed {
        c.push(warn("state", rel, "changed since the last stamp"));
    }
    for gone in &report.removed {
        let (scope, rel) = gone.split_once('/').expect("scope/rel");
        c.push(warn(
            "state",
            &format!("{scope}/state.json"),
            format!("stamps {rel}, which is gone"),
        ));
    }
}

fn budget(c: &mut Ctx) {
    let mut found = vec![];
    for (file, default) in BUDGETS {
        let Some(e) = c.id.entry(&format!("soul/{file}")) else {
            continue;
        };
        let limit = c.id.budget_override(file).unwrap_or(*default);
        let words = markdown::word_count(&e.text);
        if words > limit {
            found.push(warn(
                "budget",
                &e.rel,
                format!("{words} words, soft budget {limit}"),
            ));
        }
    }
    c.out.extend(found);
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

fn dupes(c: &mut Ctx) {
    let authored: Vec<&Entry> =
        c.id.entries
            .iter()
            .filter(|e| !e.is_generated() && e.kind != Kind::Example)
            .collect();
    let mut owners: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in authored.iter().enumerate() {
        for s in shingles(e.body()) {
            owners.entry(s).or_default().push(i);
        }
    }
    // Per file pair: how many windows they share, and the first shared one to quote.
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
        c.push(warn(
            "dupes",
            &authored[a].rel,
            format!(
                "shares text with {} (\"{passage}\", {n} overlapping {DUPE_WORDS}-word {windows}); keep it in one home and link",
                authored[b].rel
            ),
        ));
    }
}

fn orphans(c: &mut Ctx) {
    let found: Vec<Diagnostic> = crate::find::orphans(c.id)
        .into_iter()
        .map(|i| warn("orphans", &c.id.entries[i].rel, "zero inbound links"))
        .collect();
    c.out.extend(found);
}

fn soul_evidence(c: &mut Ctx) {
    let Some(history) = state::doc(c.id, "soul")["history"].as_array() else {
        return;
    };
    let mut found = vec![];
    for (n, record) in history.iter().enumerate() {
        let evidence: std::collections::BTreeSet<usize> = record
            .get("evidence")
            .and_then(Json::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Json::as_str)
                    .flat_map(|v| link_hits(c, v))
                    .collect()
            })
            .unwrap_or_default();
        let defining = evidence.iter().any(|&i| {
            let e = &c.id.entries[i];
            e.kind == Kind::Event && matches!(e.field("defining"), Some(Yaml::Bool(true)))
        });
        if evidence.len() < 2 && !defining {
            found.push(warn(
                "soul-evidence",
                "soul/state.json",
                format!("history[{n}] needs two evidence links or one defining event"),
            ));
        }
    }
    c.out.extend(found);
}

fn review_due(c: &mut Ctx) {
    let r = state::review(c.id);
    let msg = match (&r.last_reviewed, r.days_since) {
        (None, _) => {
            Some("never reviewed; run `owo review` when there is something to compile".to_string())
        }
        (Some(_), None) => Some("review.last_reviewed is not an RFC 3339 timestamp".to_string()),
        (Some(_), Some(days)) => r.due.then(|| {
            format!(
                "review is due: {days} days since the last review (every {})",
                r.every_days
            )
        }),
    };
    if let Some(m) = msg {
        c.push(Diagnostic::new(
            "review-due",
            Level::Info,
            "soul/state.json",
            m,
        ));
    }
}

/// `[[kb:<p>:<path>#<Section>]]` links: the project must exist; when its KB folder exists,
/// the file and section must too. A KB that is not checked out is skipped.
fn kb_links(c: &mut Ctx) {
    let mut found = vec![];
    for e in c.id.entries.iter().filter(|e| !e.is_generated()) {
        for (line, link) in crate::find::raw_links(e) {
            let full = match &link.section {
                Some(s) => format!("{}#{s}", link.target),
                None => link.target.clone(),
            };
            match crate::find::kb_link(c.id, &full) {
                None => {}
                Some(Err(msg)) => found.push(err("kb", &e.rel, msg).at(line)),
                Some(Ok(k)) if !k.root_exists => {}
                Some(Ok(k)) if !k.file_exists => found.push(
                    warn(
                        "kb",
                        &e.rel,
                        format!(
                            "[[{}]]: {} not found",
                            link.target,
                            k.file.unwrap_or_default()
                        ),
                    )
                    .at(line),
                ),
                Some(Ok(k)) if !k.section_exists => found.push(
                    warn(
                        "kb",
                        &e.rel,
                        format!(
                            "[[{}]]: no section {:?}",
                            link.target,
                            k.section.unwrap_or_default()
                        ),
                    )
                    .at(line),
                ),
                Some(Ok(_)) => {}
            }
        }
    }
    c.out.extend(found);
}
