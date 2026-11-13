//! Loads one identity folder into memory: manifest, state files and every Markdown entry.

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

/// Life folders that hold one entry type each, with the type they hold.
pub const LIFE_CATEGORIES: &[(&str, Kind)] = &[
    ("characters", Kind::Character),
    ("locations", Kind::Location),
    ("cultures", Kind::Culture),
    ("factions", Kind::Faction),
    ("events", Kind::Event),
    ("eras", Kind::Era),
    ("threads", Kind::Thread),
    ("conflicts", Kind::Conflict),
    ("lessons", Kind::Lesson),
    ("writing", Kind::Writing),
    ("log", Kind::Log),
    ("journal", Kind::Journal),
];

/// What a file is, decided by its path.
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
    WorkReadme,
    Generated,
    Unknown,
}

impl Kind {
    pub const ALL: [Kind; 20] = [
        Kind::Soul,
        Kind::Example,
        Kind::Character,
        Kind::Location,
        Kind::LocationDetail,
        Kind::Culture,
        Kind::Faction,
        Kind::Event,
        Kind::Era,
        Kind::Thread,
        Kind::Conflict,
        Kind::Lesson,
        Kind::Writing,
        Kind::Log,
        Kind::Journal,
        Kind::Project,
        Kind::Note,
        Kind::WorkReadme,
        Kind::Generated,
        Kind::Unknown,
    ];

    /// The kind whose `type:` value is `name`.
    pub fn from_type(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.type_name() == Some(name))
    }

    /// The `type:` value an entry of this kind carries in frontmatter.
    pub fn type_name(self) -> Option<&'static str> {
        Some(match self {
            Kind::Character => "character",
            Kind::Location => "location",
            Kind::LocationDetail => "location-detail",
            Kind::Culture => "culture",
            Kind::Faction => "faction",
            Kind::Event => "event",
            Kind::Era => "era",
            Kind::Thread => "thread",
            Kind::Conflict => "conflict",
            Kind::Lesson => "lesson",
            Kind::Writing => "writing",
            Kind::Log => "log",
            Kind::Journal => "journal",
            Kind::Project => "project",
            Kind::Note => "note",
            _ => return None,
        })
    }

    /// The name of this kind in output and `--type`: its `type:` value, else a name for the file kind.
    pub fn name(self) -> &'static str {
        self.type_name().unwrap_or(match self {
            Kind::Soul => "soul",
            Kind::Example => "example",
            Kind::WorkReadme => "work-readme",
            Kind::Generated => "generated",
            _ => "unknown",
        })
    }

    /// Every kind name, for `--type` value lists.
    pub fn names() -> impl Iterator<Item = &'static str> {
        Kind::ALL.into_iter().map(Kind::name)
    }

    /// Entries with YAML frontmatter.
    pub fn has_frontmatter(self) -> bool {
        self.type_name().is_some()
    }

    /// Files named `YYYY-MM-DD.md` (log) or `YYYY-MM-DD_<slug>.md`.
    pub fn dated(self) -> Option<bool> {
        match self {
            Kind::Log => Some(false),
            Kind::Journal | Kind::Writing | Kind::Note => Some(true),
            _ => None,
        }
    }

    /// The identity-relative path `owo add` gives a new entry; `classify` maps it back to this
    /// kind. None for a kind `owo add` does not create, a location detail without its parent
    /// location slug, or a note without its project.
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
                let (dir, _) = LIFE_CATEGORIES.iter().find(|(_, k)| *k == self)?;
                match self.dated() {
                    Some(true) => format!("life/{dir}/{date}_{slug}.md"),
                    _ => format!("life/{dir}/{slug}.md"),
                }
            }
        })
    }
}

/// One Markdown file of the identity.
#[derive(Debug)]
pub struct Entry {
    /// Path relative to the identity folder, with `/` separators.
    pub rel: String,
    pub kind: Kind,
    pub text: String,
    pub frontmatter: Option<Fields>,
    pub frontmatter_error: Option<String>,
    /// True when the file is not valid UTF-8; `text` holds U+FFFD in place of the bad bytes.
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
        match self.field(key)? {
            Json::String(s) => Some(s.clone()),
            Json::Number(n) => Some(n.to_string()),
            Json::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    /// The title, else the stem key.
    pub fn label(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.stem_key())
    }

    /// Slug-like key from the path: the folder name for `overview.md` and `project.md`,
    /// else the file stem.
    pub fn stem_key(&self) -> String {
        let path = Path::new(&self.rel);
        if matches!(
            path.file_name().and_then(|f| f.to_str()),
            Some("overview.md" | "project.md")
        ) {
            if let Some(parent) = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|f| f.to_str())
            {
                return parent.to_string();
            }
        }
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string()
    }

    pub fn is_generated(&self) -> bool {
        self.kind == Kind::Generated
    }

    /// True when the body holds only its title, blank lines or empty bullets, as a fresh log does.
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
    /// `identity.json`, or Null when it is missing or does not parse.
    pub manifest: Json,
    pub manifest_error: Option<String>,
    /// `soul`, `life` and `work` state.json in `state::SCOPES` order, read through `state::doc`.
    pub state: [Json; 3],
    pub entries: Vec<Entry>,
    /// Lowercase link key to the entries it can point at; see `resolve_link`.
    pub keys: BTreeMap<String, Vec<usize>>,
    /// The home folder for `~/` in a project KB path; the caller sets it.
    pub home: Option<PathBuf>,
}

pub fn classify(rel: &str) -> Kind {
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        ["life" | "work", "_index.md"] | ["life", "timeline.md"] => Kind::Generated,
        ["life", category, "_index.md"] if LIFE_CATEGORIES.iter().any(|(c, _)| c == category) => {
            Kind::Generated
        }
        ["soul", file] if SOUL_FILES.contains(file) => Kind::Soul,
        ["soul", "examples", _] => Kind::Example,
        ["work", "README.md"] => Kind::WorkReadme,
        ["work", "projects", _, "project.md"] => Kind::Project,
        ["work", "projects", _, "notes", _] => Kind::Note,
        ["life", "locations", _, "overview.md"] => Kind::Location,
        ["life", "locations", _, _] => Kind::LocationDetail,
        ["life", category, _] => LIFE_CATEGORIES
            .iter()
            .find(|(c, _)| c == category)
            .map(|(_, k)| *k)
            .filter(|k| *k != Kind::Location)
            .unwrap_or(Kind::Unknown),
        _ => Kind::Unknown,
    }
}

fn parse_entry(rel: String, text: String, lossy: bool) -> Entry {
    let kind = classify(&rel);
    let split = markdown::split(&text);
    let mut frontmatter = None;
    let mut frontmatter_error = None;
    if let Some(fm) = split.frontmatter {
        match markdown::parse_frontmatter(fm) {
            Ok(m) => frontmatter = Some(m),
            Err(e) => frontmatter_error = Some(e),
        }
    }
    let heading = markdown::first_heading(split.body);
    let mut entry = Entry {
        rel,
        kind,
        text: String::new(),
        frontmatter,
        frontmatter_error,
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

/// A JSON file, or None when it is missing or does not parse.
pub fn read_json(path: &Path) -> Option<Json> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// A JSON object file for a command that rewrites it: None when it is missing, an error when
/// it does not parse or is not an object, so the rewrite does not drop its contents.
pub fn read_object(path: &Path) -> Result<Option<Json>> {
    let text = match fs::read_to_string(path) {
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

/// Writes pretty JSON with a trailing newline through a temporary file and a rename, so a
/// killed write never leaves a cut-off file.
pub fn write_json(path: &Path, doc: &Json) -> Result<()> {
    let text = format!("{}\n", serde_json::to_string_pretty(doc)?);
    let tmp = path.with_extension("json.tmp");
    write_file(&tmp, text)?;
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// Writes a file, creating its folders first.
pub fn write_file(path: &Path, text: impl AsRef<[u8]>) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

impl Identity {
    pub fn load(root: &Path, slug: &str) -> Result<Self> {
        let dir = root.join(slug);
        let (manifest, manifest_error) = match fs::read_to_string(dir.join("identity.json")) {
            Ok(text) => match serde_json::from_str::<Json>(&text) {
                Ok(v) => (v, None),
                Err(e) => (Json::Null, Some(e.to_string())),
            },
            Err(_) => (Json::Null, None),
        };
        let mut entries = vec![];
        for item in WalkDir::new(&dir).sort_by_file_name() {
            let item = item
                .map_err(std::io::Error::from)
                .with_context(|| format!("walking {}", dir.display()))?;
            if !item.file_type().is_file()
                || item.path().extension().and_then(|e| e.to_str()) != Some("md")
            {
                continue;
            }
            let rel = item
                .path()
                .strip_prefix(&dir)
                .unwrap_or(item.path())
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = fs::read(item.path())
                .with_context(|| format!("reading {}", item.path().display()))?;
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

    /// Map from lowercase link key to the entries it can point at.
    fn link_keys(entries: &[Entry]) -> BTreeMap<String, Vec<usize>> {
        let mut keys: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut add = |key: String, idx: usize| {
            let key = key.trim().to_lowercase();
            if key.is_empty() {
                return;
            }
            let list = keys.entry(key).or_default();
            if !list.contains(&idx) {
                list.push(idx);
            }
        };
        for (idx, e) in entries.iter().enumerate() {
            if e.is_generated() {
                continue;
            }
            if e.kind == Kind::Soul {
                add(
                    e.rel
                        .trim_start_matches("soul/")
                        .trim_end_matches(".md")
                        .to_string(),
                    idx,
                );
            }
            if let Some(t) = &e.title {
                add(t.clone(), idx);
            }
            for a in &e.aliases {
                add(a.clone(), idx);
            }
            add(e.stem_key(), idx);
            add(e.rel.trim_end_matches(".md").to_string(), idx);
        }
        keys
    }

    /// Resolves a link target to entry indexes. Tries the raw key (a path may end in `.md`),
    /// then its slug.
    pub fn resolve_link(&self, target: &str) -> Vec<usize> {
        let raw = target.trim().to_lowercase();
        let raw = raw.strip_suffix(".md").unwrap_or(&raw).to_string();
        if let Some(hit) = self.keys.get(&raw) {
            return hit.clone();
        }
        self.keys.get(&slugify(target)).cloned().unwrap_or_default()
    }

    /// Budget overrides from `identity.json` → `budgets`.
    pub fn budget_override(&self, file: &str) -> Option<usize> {
        self.manifest["budgets"][file].as_u64().map(|n| n as usize)
    }

    /// The display name from `identity.json`, else the slug.
    pub fn name(&self) -> &str {
        self.manifest["name"].as_str().unwrap_or(&self.slug)
    }

    /// The `project.md` of a project.
    pub fn project(&self, slug: &str) -> Option<&Entry> {
        self.entry(&format!("work/projects/{slug}/project.md"))
    }

    /// Project slugs, in file-name order.
    pub fn projects(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter(|e| e.kind == Kind::Project)
            .map(|e| e.stem_key())
            .collect()
    }

    /// The notes of a project, newest first.
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
        for (name, _) in crate::templates::ENTRY_TEMPLATES {
            let kind = Kind::from_type(name).expect("an entry type");
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
        assert_eq!(classify("work/README.md"), Kind::WorkReadme);
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
}
