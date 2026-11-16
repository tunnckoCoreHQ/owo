use crate::Named;
use regex::{Captures, Regex};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

macro_rules! template {
    ($f:expr) => {
        include_str!(concat!("../../../templates/", $f))
    };
}
pub(crate) use template;

pub const IDENTITY_FILES: &[(&str, &str)] = &[
    ("identity.json", template!("identity.json")),
    ("persona/soul.json", template!("persona/soul.json")),
    ("persona/IDENTITY.md", template!("persona/IDENTITY.md")),
    ("persona/SOUL.md", template!("persona/SOUL.md")),
    ("persona/VOICE.md", template!("persona/VOICE.md")),
    ("persona/LOOK.md", template!("persona/LOOK.md")),
    ("persona/MEMORY.md", template!("persona/MEMORY.md")),
    (
        "persona/examples/good.md",
        template!("persona/examples-good.md"),
    ),
    (
        "persona/examples/bad.md",
        template!("persona/examples-bad.md"),
    ),
    (
        "persona/examples/greetings.md",
        template!("persona/examples-greetings.md"),
    ),
    ("work/README.md", template!("work-README.md")),
];

pub fn all() -> Vec<(&'static str, String, &'static str)> {
    IDENTITY_FILES
        .iter()
        .map(|(n, t)| (*n, n.to_string(), *t))
        .chain(crate::identity::entry_types().map(|k| {
            let n = k.name();
            let t = k.template().expect("an entry type has a template");
            (n, format!("{n}.md"), t)
        }))
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
        for kind in crate::identity::entry_types() {
            let (name, text) = (kind.name(), kind.template().expect("a template"));
            assert!(text.starts_with("---\n"), "{name}");
            assert!(text.contains(&format!("type: {name}\n")), "{name}");
        }
    }
}
