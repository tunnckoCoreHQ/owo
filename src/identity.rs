//! Loads one identity folder into memory: manifest, state files and every Markdown entry.

use crate::markdown;
use crate::markdown::Fields;
use crate::slug::slugify;
use anyhow::{Context, Result};
use serde_json::Value as Json;
use serde_json::Value as Yaml;
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
    pub title: Option<String>,
    pub aliases: Vec<String>,
}

impl Entry {
    pub fn body(&self) -> &str {
        markdown::split(&self.text).body
    }

    pub fn field(&self, key: &str) -> Option<&Yaml> {
        self.frontmatter.as_ref()?.get(key)
    }

    pub fn field_str(&self, key: &str) -> Option<String> {
        match self.field(key)? {
            Yaml::String(s) => Some(s.clone()),
            Yaml::Number(n) => Some(n.to_string()),
            Yaml::Bool(b) => Some(b.to_string()),
            _ => None,
        }
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
    pub manifest: Option<Json>,
    pub manifest_error: Option<String>,
    pub entries: Vec<Entry>,
}

pub fn classify(rel: &str) -> Kind {
    let parts: Vec<&str> = rel.split('/').collect();
    let name = *parts.last().unwrap_or(&"");
    if name == "_index.md" || rel == "life/timeline.md" {
        return Kind::Generated;
    }
    match parts.as_slice() {
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

fn parse_entry(rel: String, text: String) -> Entry {
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
        title: None,
        aliases: vec![],
    };
    entry.title = entry
        .field_str("title")
        .filter(|t| !t.trim().is_empty())
        .or(heading);
    if let Some(Yaml::Array(items)) = entry.field("aliases") {
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

/// Writes pretty JSON with a trailing newline.
pub fn write_json(path: &Path, doc: &Json) -> Result<()> {
    let text = format!("{}\n", serde_json::to_string_pretty(doc)?);
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

impl Identity {
    pub fn load(root: &Path, slug: &str) -> Result<Self> {
        let dir = root.join(slug);
        if !dir.is_dir() {
            anyhow::bail!("identity folder not found: {}", dir.display());
        }
        let (manifest, manifest_error) = match fs::read_to_string(dir.join("identity.json")) {
            Ok(text) => match serde_json::from_str::<Json>(&text) {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e.to_string())),
            },
            Err(_) => (None, None),
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
            let text = fs::read_to_string(item.path())
                .with_context(|| format!("reading {}", item.path().display()))?;
            entries.push(parse_entry(rel, text));
        }
        Ok(Self {
            slug: slug.to_string(),
            dir,
            manifest,
            manifest_error,
            entries,
        })
    }

    pub fn read_json(&self, rel: &str) -> Option<Json> {
        read_json(&self.dir.join(rel))
    }

    pub fn entry(&self, rel: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.rel == rel)
    }

    /// Map from lowercase link key to the entries it can point at.
    pub fn link_keys(&self) -> BTreeMap<String, Vec<usize>> {
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
        for (idx, e) in self.entries.iter().enumerate() {
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
    pub fn resolve_link(&self, keys: &BTreeMap<String, Vec<usize>>, target: &str) -> Vec<usize> {
        // A file path inside the identity folder (absolute, or relative to the working directory)
        // resolves like its identity-relative path.
        let looks_like_path = target.contains('/') || target.ends_with(".md");
        let inside = looks_like_path
            .then(|| fs::canonicalize(target.trim()).ok())
            .flatten()
            .and_then(|p| {
                let dir = fs::canonicalize(&self.dir).ok()?;
                Some(
                    p.strip_prefix(dir)
                        .ok()?
                        .to_string_lossy()
                        .replace('\\', "/"),
                )
            });
        let target = inside.as_deref().unwrap_or(target);
        let raw = target.trim().to_lowercase();
        let raw = raw.strip_suffix(".md").unwrap_or(&raw).to_string();
        if let Some(hit) = keys.get(&raw) {
            return hit.clone();
        }
        keys.get(&slugify(target)).cloned().unwrap_or_default()
    }

    /// Budget overrides from `identity.json` → `budgets`.
    pub fn budget_override(&self, file: &str) -> Option<usize> {
        self.manifest
            .as_ref()?
            .get("budgets")?
            .get(file)?
            .as_u64()
            .map(|n| n as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
