//! Built-in templates, embedded in the binary. `<root>/.templates/<file>` overrides one.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Templates used by `owo new`, keyed by their identity-relative output path.
pub const IDENTITY_FILES: &[(&str, &str)] = &[
    ("identity.json", include_str!("../templates/identity.json")),
    (
        "soul/soul.json",
        include_str!("../templates/soul/soul.json"),
    ),
    (
        "soul/IDENTITY.md",
        include_str!("../templates/soul/IDENTITY.md"),
    ),
    ("soul/SOUL.md", include_str!("../templates/soul/SOUL.md")),
    ("soul/VOICE.md", include_str!("../templates/soul/VOICE.md")),
    ("soul/LOOK.md", include_str!("../templates/soul/LOOK.md")),
    (
        "soul/MEMORY.md",
        include_str!("../templates/soul/MEMORY.md"),
    ),
    (
        "soul/examples/good.md",
        include_str!("../templates/soul/examples-good.md"),
    ),
    (
        "soul/examples/bad.md",
        include_str!("../templates/soul/examples-bad.md"),
    ),
    (
        "soul/examples/greetings.md",
        include_str!("../templates/soul/examples-greetings.md"),
    ),
    (
        "work/README.md",
        include_str!("../templates/work-README.md"),
    ),
];

/// Entry templates, keyed by entry type.
pub const ENTRY_TEMPLATES: &[(&str, &str)] = &[
    (
        "character",
        include_str!("../templates/entries/character.md"),
    ),
    ("location", include_str!("../templates/entries/location.md")),
    (
        "location-detail",
        include_str!("../templates/entries/location-detail.md"),
    ),
    ("culture", include_str!("../templates/entries/culture.md")),
    ("faction", include_str!("../templates/entries/faction.md")),
    ("era", include_str!("../templates/entries/era.md")),
    ("event", include_str!("../templates/entries/event.md")),
    ("thread", include_str!("../templates/entries/thread.md")),
    ("conflict", include_str!("../templates/entries/conflict.md")),
    ("lesson", include_str!("../templates/entries/lesson.md")),
    ("writing", include_str!("../templates/entries/writing.md")),
    ("log", include_str!("../templates/entries/log.md")),
    ("journal", include_str!("../templates/entries/journal.md")),
    ("project", include_str!("../templates/entries/project.md")),
    ("note", include_str!("../templates/entries/note.md")),
];

/// Every template as (name shown by `owo templates`, file name looked up in `.templates/`, built-in text).
pub fn all() -> Vec<(&'static str, String, &'static str)> {
    IDENTITY_FILES
        .iter()
        .map(|(n, t)| (*n, n.to_string(), *t))
        .chain(
            ENTRY_TEMPLATES
                .iter()
                .map(|(n, t)| (*n, format!("{n}.md"), *t)),
        )
        .collect()
}

/// The override for a template: `<root>/.templates/<file>`, where `<file>` is the name from `all`.
pub fn override_path(root: &Path, file: &str) -> Option<PathBuf> {
    Some(root.join(".templates").join(file)).filter(|p| p.is_file())
}

/// Returns the template text for a name, preferring its override in `<root>/.templates/`.
pub fn get(root: &Path, name: &str, builtin: &str) -> String {
    override_path(root, name)
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(|| builtin.to_string())
}

/// Replaces `{{key}}` placeholders. Keys ending in `_yaml` get double-quote YAML escaping, keys
/// ending in `_json` JSON string escaping.
pub fn fill(template: &str, values: &BTreeMap<&str, String>) -> String {
    let mut out = template.to_string();
    for (key, value) in values {
        out = out.replace(&format!("{{{{{key}}}}}"), value);
        out = out.replace(&format!("{{{{{key}_yaml}}}}"), &yaml_escape(value));
        let json = serde_json::to_string(value).expect("a string serializes");
        out = out.replace(&format!("{{{{{key}_json}}}}"), &json[1..json.len() - 1]);
    }
    out
}

/// Escapes a value for a double-quoted YAML string.
pub fn yaml_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_and_escapes() {
        let mut v = BTreeMap::new();
        v.insert("title", "Say \"hi\"".to_string());
        assert_eq!(
            fill("t: \"{{title_yaml}}\" # {{title}}", &v),
            "t: \"Say \\\"hi\\\"\" # Say \"hi\""
        );
    }

    #[test]
    fn every_entry_template_has_frontmatter() {
        for (name, text) in ENTRY_TEMPLATES {
            assert!(text.starts_with("---\n"), "{name}");
            assert!(text.contains(&format!("type: {name}\n")), "{name}");
        }
    }
}
