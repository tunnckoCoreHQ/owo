//! Markdown helpers: frontmatter split, sections, wiki-links, word counts.

use regex::Regex;
use std::sync::OnceLock;

/// A document split into optional YAML frontmatter and body.
pub struct Split<'a> {
    pub frontmatter: Option<&'a str>,
    pub body: &'a str,
    /// 1-based line number where the body starts in the file.
    pub body_line: usize,
}

pub fn split(text: &str) -> Split<'_> {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return Split {
            frontmatter: None,
            body: text,
            body_line: 1,
        };
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let fm = &rest[..offset];
            let body = &rest[offset + line.len()..];
            let body_line = 2 + fm.lines().count() + 1;
            return Split {
                frontmatter: Some(fm),
                body,
                body_line,
            };
        }
        offset += line.len();
    }
    Split {
        frontmatter: None,
        body: text,
        body_line: 1,
    }
}

/// First `# ` heading in the body.
pub fn first_heading(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("# ").map(|h| h.trim().to_string()))
}

/// Lines of the `## <name>` section, without the heading, up to the next `#`/`##` heading.
pub fn section<'a>(body: &'a str, name: &str) -> Option<Vec<(usize, &'a str)>> {
    let mut found = None;
    let mut lines = vec![];
    for (i, line) in body.lines().enumerate() {
        if let Some(h) = line.strip_prefix("## ") {
            if found.is_some() {
                break;
            }
            if h.trim().eq_ignore_ascii_case(name) {
                found = Some(i);
            }
            continue;
        }
        if line.starts_with("# ") && found.is_some() {
            break;
        }
        if found.is_some() {
            lines.push((i, line));
        }
    }
    found.map(|_| lines)
}

/// A `[[target#section|label]]` link with its 0-based line in the body.
#[derive(Debug, Clone)]
pub struct Link {
    pub target: String,
    /// Text after `#`, if any.
    pub section: Option<String>,
    pub line: usize,
}

fn link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\[\[([^\[\]|#]*)(?:#([^\[\]|]*))?(?:\|[^\[\]]*)?\]\]").expect("valid regex")
    })
}

/// Wiki-links in text, skipping fenced code blocks and inline code spans.
pub fn links(text: &str) -> Vec<Link> {
    let mut out = vec![];
    let mut fenced = false;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let cleaned = strip_inline_code(line);
        for cap in link_re().captures_iter(&cleaned) {
            let target = cap[1].trim().to_string();
            let section = cap.get(2).map(|m| m.as_str().trim().to_string());
            out.push(Link {
                target,
                section,
                line: i,
            });
        }
    }
    out
}

/// Wiki-links inside a YAML string value, for fields like `parent` or `evidence`.
pub fn links_in_value(value: &str) -> Vec<Link> {
    links(value)
}

fn strip_inline_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for ch in line.chars() {
        if ch == '`' {
            in_code = !in_code;
            continue;
        }
        if !in_code {
            out.push(ch);
        }
    }
    out
}

/// Word count of prose, skipping headings and fenced code.
pub fn word_count(body: &str) -> usize {
    let mut fenced = false;
    body.lines()
        .filter(|l| {
            if l.trim_start().starts_with("```") {
                fenced = !fenced;
                return false;
            }
            !fenced && !l.starts_with('#')
        })
        .map(|l| l.split_whitespace().count())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter() {
        let s = split("---\ntitle: A\ntype: x\n---\n\n# A\nbody\n");
        assert_eq!(s.frontmatter, Some("title: A\ntype: x\n"));
        assert_eq!(s.body, "\n# A\nbody\n");
        assert_eq!(s.body_line, 5);
        assert_eq!(first_heading(s.body).as_deref(), Some("A"));
        assert!(split("# plain\n").frontmatter.is_none());
    }

    #[test]
    fn finds_links_outside_code() {
        let text = "See [[The Café|café]] and [[Joe#Identifiers]].\n`[[Not A Link]]`\n```\n[[Fenced]]\n```\n";
        let found = links(text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].target, "The Café");
        assert_eq!(found[1].target, "Joe");
    }

    #[test]
    fn reads_sections() {
        let body = "# Joe\n\n## Key facts\n- a\n\n## Identifiers\n- born: 1990-01-01\n- x: @joe\n\n## Sources\n";
        let ids = section(body, "Identifiers").unwrap();
        let values: Vec<&str> = ids
            .iter()
            .map(|(_, l)| *l)
            .filter(|l| !l.trim().is_empty())
            .collect();
        assert_eq!(values, vec!["- born: 1990-01-01", "- x: @joe"]);
        assert!(section(body, "Missing").is_none());
    }

    #[test]
    fn counts_words() {
        assert_eq!(
            word_count("# Title\n\none two three\n```\nskip me\n```\nfour\n"),
            4
        );
    }
}
