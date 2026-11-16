use crate::markdown;
use crate::markdown::Fields;
use crate::slug::slugify;
use anyhow::{Context, Result};
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const SOUL_FILES: &[&str] = &["IDENTITY.md", "SOUL.md", "VOICE.md", "LOOK.md", "MEMORY.md"];

struct KindRow {
    kind: Kind,
    name: &'static str,
    template: Option<&'static str>,
    category: Option<&'static str>,
}

macro_rules! typed {
    ($kind:ident, $name:literal, $category:expr) => {
        KindRow {
            kind: Kind::$kind,
            name: $name,
            template: Some(crate::templates::template!(concat!(
                "entries/", $name, ".md"
            ))),
            category: $category,
        }
    };
}

const fn untyped(kind: Kind, name: &'static str) -> KindRow {
    KindRow {
        kind,
        name,
        template: None,
        category: None,
    }
}

const KIND_ROWS: [KindRow; Kind::Unknown as usize + 1] = [
    untyped(Kind::Soul, "soul"),
    untyped(Kind::Example, "example"),
    typed!(Character, "character", Some("characters")),
    typed!(Location, "location", Some("locations")),
    typed!(LocationDetail, "location-detail", None),
    typed!(Culture, "culture", Some("cultures")),
    typed!(Faction, "faction", Some("factions")),
    typed!(Event, "event", Some("events")),
    typed!(Era, "era", Some("eras")),
    typed!(Thread, "thread", Some("threads")),
    typed!(Conflict, "conflict", Some("conflicts")),
    typed!(Lesson, "lesson", Some("lessons")),
    typed!(Writing, "writing", Some("writing")),
    typed!(Log, "log", Some("log")),
    typed!(Journal, "journal", Some("journal")),
    typed!(Project, "project", None),
    typed!(Note, "note", None),
    untyped(Kind::WorkProfile, "work-profile"),
    untyped(Kind::Generated, "generated"),
    untyped(Kind::Unknown, "unknown"),
];

const _: () = {
    let mut i = 0;
    while i < KIND_ROWS.len() {
        assert!(
            KIND_ROWS[i].kind as usize == i,
            "KIND_ROWS is in the order of Kind"
        );
        i += 1;
    }
};

pub fn life_categories() -> impl Iterator<Item = (&'static str, Kind)> {
    KIND_ROWS.iter().filter_map(|r| Some((r.category?, r.kind)))
}

pub fn entry_types() -> impl Iterator<Item = Kind> {
    KIND_ROWS
        .iter()
        .filter(|r| r.template.is_some())
        .map(|r| r.kind)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Soul,
    Example,
    Character,
    Location,
    LocationDetail,
    Culture,
    Faction,
    Event,
    Era,
    Thread,
    Conflict,
    Lesson,
    Writing,
    Log,
    Journal,
    Project,
    Note,
    WorkProfile,
    Generated,
    Unknown,
}

impl crate::Named for Kind {
    fn rows() -> impl Iterator<Item = (Kind, &'static str)> {
        KIND_ROWS.iter().map(|r| (r.kind, r.name))
    }
}

impl Kind {
    fn kind_row(self) -> &'static KindRow {
        &KIND_ROWS[self as usize]
    }

    pub fn type_name(self) -> Option<&'static str> {
        let r = self.kind_row();
        r.template.map(|_| r.name)
    }

    pub fn has_frontmatter(self) -> bool {
        self.kind_row().template.is_some()
    }

    pub fn template(self) -> Option<&'static str> {
        self.kind_row().template
    }

    pub fn dated(self) -> Option<bool> {
        match self {
            Kind::Log => Some(false),
            Kind::Journal | Kind::Writing | Kind::Note => Some(true),
            _ => None,
        }
    }

    pub fn rel(
        self,
        slug: &str,
        date: &str,
        parent: Option<&str>,
        project: Option<&str>,
    ) -> Option<String> {
        Some(match self {
            Kind::Location => format!("life/locations/{slug}/overview.md"),
            Kind::LocationDetail => format!("life/locations/{}/{slug}.md", parent?),
            Kind::Project => format!("work/projects/{slug}/project.md"),
            Kind::Note => format!("work/projects/{}/notes/{date}_{slug}.md", project?),
            Kind::Log => format!("life/log/{date}.md"),
            _ => {
                let dir = self.kind_row().category?;
                match self.dated() {
                    Some(true) => format!("life/{dir}/{date}_{slug}.md"),
                    _ => format!("life/{dir}/{slug}.md"),
                }
            }
        })
    }
}

#[derive(Debug)]
pub struct Entry {
    pub rel: String,
    pub kind: Kind,
    pub text: String,
    pub frontmatter: Option<Fields>,
    pub frontmatter_error: Option<String>,
    pub lossy: bool,
    pub title: Option<String>,
    pub aliases: Vec<String>,
}

impl Entry {
    pub fn body(&self) -> &str {
        markdown::split(&self.text).body
    }

    pub fn field(&self, key: &str) -> Option<&Json> {
        self.frontmatter.as_ref()?.get(key)
    }

    pub fn field_str(&self, key: &str) -> Option<String> {
        self.field(key).and_then(crate::request::scalar)
    }

    pub fn label(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.stem_key())
    }

    pub fn stem_key(&self) -> String {
        let mut parts = self.rel.rsplit('/');
        let file = parts.next().unwrap_or_default();
        match parts.next() {
            Some(dir) if matches!(file, "overview.md" | "project.md") => dir,
            _ => file.strip_suffix(".md").unwrap_or(file),
        }
        .to_string()
    }

    pub fn is_generated(&self) -> bool {
        self.kind == Kind::Generated
    }

    pub fn is_blank(&self) -> bool {
        self.body().lines().all(|l| {
            let t = l.trim();
            t.is_empty() || t == "-" || t.starts_with("# ")
        })
    }
}

pub struct Identity {
    pub slug: String,
    pub dir: PathBuf,
    pub manifest: Json,
    pub manifest_error: Option<String>,
    pub state: [Json; 3],
    pub entries: Vec<Entry>,
    pub keys: BTreeMap<String, Vec<usize>>,
    pub project: Option<String>,
    pub home: Option<PathBuf>,
    pub cwd: PathBuf,
}

pub fn classify(rel: &str) -> Kind {
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        ["life" | "work", "_index.md"] | ["life", "timeline.md"] => Kind::Generated,
        ["life", category, "_index.md"] if life_categories().any(|(c, _)| c == *category) => {
            Kind::Generated
        }
        ["soul", file] if SOUL_FILES.contains(file) => Kind::Soul,
        ["soul", "examples", _] => Kind::Example,
        ["work", "README.md"] => Kind::WorkProfile,
        ["work", "projects", _, "project.md"] => Kind::Project,
        ["work", "projects", _, "notes", _] => Kind::Note,
        ["life", "locations", _, "overview.md"] => Kind::Location,
        ["life", "locations", _, _] => Kind::LocationDetail,
        ["life", category, _] => life_categories()
            .find(|(c, _)| c == category)
            .map(|(_, k)| k)
            .filter(|k| *k != Kind::Location)
            .unwrap_or(Kind::Unknown),
        _ => Kind::Unknown,
    }
}

fn parse_entry(rel: String, text: String, lossy: bool) -> Entry {
    let kind = classify(&rel);
    let split = markdown::split(&text);
    let parsed = split.frontmatter.map(markdown::parse_frontmatter);
    let heading = markdown::first_heading(split.body);
    let mut entry = Entry {
        rel,
        kind,
        text: String::new(),
        frontmatter_error: parsed.as_ref().and_then(|p| p.as_ref().err()).cloned(),
        frontmatter: parsed.and_then(Result::ok),
        lossy,
        title: None,
        aliases: vec![],
    };
    entry.title = entry
        .field_str("title")
        .filter(|t| !t.trim().is_empty())
        .or(heading);
    if let Some(Json::Array(items)) = entry.field("aliases") {
        entry.aliases = items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .filter(|s| !s.trim().is_empty())
            .collect();
    }
    entry.text = text;
    entry
}

pub fn read_json(path: &Path) -> Option<Json> {
    serde_json::from_str(&crate::write::read_to_string(path).ok()?).ok()
}

pub fn read_object(path: &Path) -> Result<Option<Json>> {
    let text = match crate::write::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let doc: Json = serde_json::from_str(&text)
        .with_context(|| format!("{} does not parse; fix it by hand", path.display()))?;
    if !doc.is_object() {
        anyhow::bail!("{} is not a JSON object; fix it by hand", path.display());
    }
    Ok(Some(doc))
}

impl Identity {
    pub fn load(root: &Path, slug: &str) -> Result<Self> {
        let dir = root.join(slug);
        let (manifest, manifest_error) =
            match crate::write::read_to_string(&dir.join("identity.json")) {
                Ok(text) => match serde_json::from_str::<Json>(&text) {
                    Ok(v) => (v, None),
                    Err(e) => (Json::Null, Some(e.to_string())),
                },
                Err(_) => (Json::Null, None),
            };
        let md = |p: &Path| p.extension().and_then(|e| e.to_str()) == Some("md");
        let mut files = BTreeMap::new();
        if !crate::write::dry() || dir.exists() {
            for item in WalkDir::new(&dir).sort_by_file_name() {
                let item = item
                    .map_err(std::io::Error::from)
                    .with_context(|| format!("walking {}", dir.display()))?;
                if !item.file_type().is_file() || !md(item.path()) {
                    continue;
                }
                let bytes = fs::read(item.path())
                    .with_context(|| format!("reading {}", item.path().display()))?;
                let rel = item.path().strip_prefix(&dir).unwrap_or(item.path());
                files.insert(rel.to_path_buf(), bytes);
            }
        }
        files.extend(
            crate::write::kept_under(&dir)
                .into_iter()
                .filter(|(rel, _)| md(rel)),
        );
        let mut entries = vec![];
        for (rel, bytes) in files {
            let rel = rel.to_string_lossy().replace('\\', "/");
            let (text, lossy) = match String::from_utf8(bytes) {
                Ok(text) => (text, false),
                Err(e) => (String::from_utf8_lossy(e.as_bytes()).into_owned(), true),
            };
            entries.push(parse_entry(rel, text, lossy));
        }
        let state = crate::state::SCOPES
            .map(|s| read_json(&dir.join(s).join("state.json")).unwrap_or(Json::Null));
        Ok(Self {
            slug: slug.to_string(),
            dir,
            manifest,
            manifest_error,
            state,
            keys: Self::link_keys(&entries),
            home: None,
            cwd: PathBuf::new(),
            project: None,
            entries,
        })
    }

    pub fn read_object(&self, rel: &str) -> Result<Option<Json>> {
        read_object(&self.dir.join(rel))
    }

    pub fn read_json(&self, rel: &str) -> Option<Json> {
        read_json(&self.dir.join(rel))
    }

    pub fn entry(&self, rel: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.rel == rel)
    }

    fn link_keys(entries: &[Entry]) -> BTreeMap<String, Vec<usize>> {
        let mut keys: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (idx, e) in entries.iter().enumerate() {
            if e.is_generated() {
                continue;
            }
            let mut add = |key: &str| {
                let key = key.trim().to_lowercase();
                if key.is_empty() {
                    return;
                }
                let list = keys.entry(key).or_default();
                if !list.contains(&idx) {
                    list.push(idx);
                }
            };
            if e.kind == Kind::Soul {
                add(e.rel.trim_start_matches("soul/").trim_end_matches(".md"));
            }
            for name in e.title.iter().chain(&e.aliases) {
                add(name);
            }
            add(&e.stem_key());
            add(e.rel.trim_end_matches(".md"));
        }
        keys
    }

    pub fn resolve_link(&self, target: &str) -> Vec<usize> {
        let raw = target.trim().to_lowercase();
        let raw = raw.strip_suffix(".md").unwrap_or(&raw).to_string();
        if let Some(hit) = self.keys.get(&raw) {
            return hit.clone();
        }
        self.keys.get(&slugify(target)).cloned().unwrap_or_default()
    }

    pub fn budget_override(&self, file: &str) -> Option<usize> {
        self.manifest["budgets"][file].as_u64().map(|n| n as usize)
    }

    pub fn name(&self) -> &str {
        self.manifest["name"].as_str().unwrap_or(&self.slug)
    }

    pub fn project(&self, slug: &str) -> Option<&Entry> {
        self.entry(&format!("work/projects/{slug}/project.md"))
    }

    pub fn projects(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| e.kind == Kind::Project)
            .map(|e| e.stem_key())
            .collect()
    }

    pub fn notes(&self, project: &str) -> Vec<&Entry> {
        let prefix = format!("work/projects/{project}/notes/");
        let mut notes: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Note && e.rel.starts_with(&prefix))
            .collect();
        notes.sort_by(|a, b| b.rel.cmp(&a.rel));
        notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_paths_classify_to_their_kind() {
        for kind in entry_types() {
            let rel = kind
                .rel("x", "2026-11-01", Some("flat"), Some("owo"))
                .expect("an add path");
            assert_eq!(classify(&rel), kind, "{rel}");
        }
    }

    #[test]
    fn classifies_paths() {
        assert_eq!(classify("soul/SOUL.md"), Kind::Soul);
        assert_eq!(classify("soul/examples/good.md"), Kind::Example);
        assert_eq!(classify("life/characters/joe.md"), Kind::Character);
        assert_eq!(classify("life/locations/flat/overview.md"), Kind::Location);
        assert_eq!(
            classify("life/locations/flat/balcony.md"),
            Kind::LocationDetail
        );
        assert_eq!(classify("life/locations/flat.md"), Kind::Unknown);
        assert_eq!(classify("life/log/2026-11-01.md"), Kind::Log);
        assert_eq!(classify("life/events/_index.md"), Kind::Generated);
        assert_eq!(classify("life/timeline.md"), Kind::Generated);
        assert_eq!(classify("work/README.md"), Kind::WorkProfile);
        assert_eq!(classify("work/projects/api/project.md"), Kind::Project);
        assert_eq!(
            classify("work/projects/api/notes/2026-11-01_x.md"),
            Kind::Note
        );
        assert_eq!(classify("life/stuff/x.md"), Kind::Unknown);
        assert_eq!(classify("life/stuff/_index.md"), Kind::Unknown);
        assert_eq!(classify("soul/_index.md"), Kind::Unknown);
        assert_eq!(classify("work/_index.md"), Kind::Generated);
    }

    #[test]
    fn a_dry_run_loads_the_files_it_kept() {
        let tmp = tempfile::TempDir::new().unwrap();
        let soul = tmp.path().join("thea/soul/SOUL.md");
        let ((), paths) = crate::write::dry_run(|| {
            crate::write::file(&soul, "# Soul\n").unwrap();
            let id = Identity::load(tmp.path(), "thea").unwrap();
            let text = id.entry("soul/SOUL.md").map(|e| e.text.as_str());
            assert_eq!(text, Some("# Soul\n"));
        });
        let thea = tmp.path().join("thea");
        assert_eq!(
            paths,
            [
                format!("{}/", thea.display()),
                format!("{}/", thea.join("soul").display()),
                soul.display().to_string()
            ]
        );
        assert!(!thea.exists());
    }
}
