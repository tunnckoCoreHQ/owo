use regex::{Captures, Regex};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

macro_rules! t {
    ($f:expr) => {
        include_str!(concat!("../../../templates/", $f))
    };
}

pub const IDENTITY_FILES: &[(&str, &str)] = &[
    ("identity.json", t!("identity.json")),
    ("soul/soul.json", t!("soul/soul.json")),
    ("soul/IDENTITY.md", t!("soul/IDENTITY.md")),
    ("soul/SOUL.md", t!("soul/SOUL.md")),
    ("soul/VOICE.md", t!("soul/VOICE.md")),
    ("soul/LOOK.md", t!("soul/LOOK.md")),
    ("soul/MEMORY.md", t!("soul/MEMORY.md")),
    ("soul/examples/good.md", t!("soul/examples-good.md")),
    ("soul/examples/bad.md", t!("soul/examples-bad.md")),
    (
        "soul/examples/greetings.md",
        t!("soul/examples-greetings.md"),
    ),
    ("work/README.md", t!("work-README.md")),
];

macro_rules! entries {
    ($($n:literal)*) => {
        &[$(($n, t!(concat!("entries/", $n, ".md")))),*]
    };
}

pub const ENTRY_TEMPLATES: &[(&str, &str)] = entries![
    "character" "location" "location-detail" "culture" "faction" "era" "event" "thread" "conflict"
    "lesson" "writing" "log" "journal" "project" "note"
];

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

pub fn override_path(root: &Path, file: &str) -> Option<PathBuf> {
    Some(root.join(".templates").join(file)).filter(|p| p.is_file())
}

pub fn get(root: &Path, name: &str, builtin: &str) -> String {
    override_path(root, name)
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(|| builtin.to_string())
}

fn placeholder_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\{\{(\w+)\}\}").expect("valid regex"))
}

pub fn fill(template: &str, values: &BTreeMap<&str, String>) -> String {
    placeholder_re()
        .replace_all(template, |cap: &Captures| {
            let key = &cap[1];
            if let Some(value) = values.get(key) {
                return value.clone();
            }
            if let Some(value) = key.strip_suffix("_yaml").and_then(|k| values.get(k)) {
                return yaml_escape(value);
            }
            if let Some(value) = key.strip_suffix("_json").and_then(|k| values.get(k)) {
                let json = serde_json::to_string(value).expect("a string serializes");
                return json[1..json.len() - 1].to_string();
            }
            cap[0].to_string()
        })
        .into_owned()
}

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
        v.insert("title", "Meet {{date}}".to_string());
        v.insert("date", "2026-01-01".to_string());
        assert_eq!(fill("{{title}} {{date}}", &v), "Meet {{date}} 2026-01-01");
    }

    #[test]
    fn every_entry_template_has_frontmatter() {
        for (name, text) in ENTRY_TEMPLATES {
            assert!(text.starts_with("---\n"), "{name}");
            assert!(text.contains(&format!("type: {name}\n")), "{name}");
        }
    }
}
