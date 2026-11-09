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

/// Frontmatter fields, keyed by name.
pub type Fields = serde_json::Map<String, serde_json::Value>;

/// Parses YAML frontmatter into fields. Empty frontmatter has no fields.
pub fn parse_frontmatter(fm: &str) -> Result<Fields, String> {
    if fm.trim().is_empty() {
        return Ok(Fields::new());
    }
    match serde_saphyr::from_str::<serde_json::Value>(fm) {
        Ok(serde_json::Value::Object(m)) => Ok(m),
        Ok(serde_json::Value::Null) => Ok(Fields::new()),
        Ok(_) => Err("frontmatter is not a mapping".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// Level and text of an ATX heading line (`# ` to `###### `).
pub fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    let text = line[level..].strip_prefix(' ')?;
    (1..=6).contains(&level).then(|| (level, text.trim()))
}

/// Lines with their 0-based index, marking each as outside fenced code (fence lines are not).
fn unfenced(body: &str) -> impl Iterator<Item = (usize, &str, bool)> {
    let mut fenced = false;
    body.lines().enumerate().map(move |(i, line)| {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            return (i, line, false);
        }
        (i, line, !fenced)
    })
}

/// The body with every heading outside fenced code one level deeper (`##` becomes `###`).
pub fn demote(body: &str) -> String {
    unfenced(body)
        .map(
            |(_, l, open)| match heading(l).filter(|(n, _)| open && *n < 6) {
                Some(_) => format!("#{l}"),
                None => l.to_string(),
            },
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// First `# ` heading in the body, outside fenced code.
pub fn first_heading(body: &str) -> Option<String> {
    unfenced(body).find_map(|(_, l, open)| match heading(l).filter(|_| open) {
        Some((1, h)) => Some(h.to_string()),
        _ => None,
    })
}

/// The section headed `<name>` at any level: its heading line and the lines below it with their
/// 0-based index, up to the next heading of the same or a higher level. Headings inside fenced
/// code do not count.
pub fn section<'a>(body: &'a str, name: &str) -> Option<(&'a str, Vec<(usize, &'a str)>)> {
    let mut found: Option<(usize, &str)> = None;
    let mut lines = vec![];
    for (i, line, open) in unfenced(body) {
        let h = heading(line).filter(|_| open);
        match (found, h) {
            (None, Some((l, text))) if text.eq_ignore_ascii_case(name) => found = Some((l, line)),
            (Some((at, _)), Some((l, _))) if l <= at => break,
            (Some(_), _) => lines.push((i, line)),
            _ => {}
        }
    }
    found.map(|(_, h)| (h, lines))
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
    for (i, line, open) in unfenced(text) {
        if !open {
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
    unfenced(body)
        .filter(|(_, l, open)| *open && !l.starts_with('#'))
        .map(|(_, l, _)| l.split_whitespace().count())
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
        let (head, ids) = section(body, "Identifiers").unwrap();
        assert_eq!(head, "## Identifiers");
        let values: Vec<&str> = ids
            .iter()
            .map(|(_, l)| *l)
            .filter(|l| !l.trim().is_empty())
            .collect();
        assert_eq!(values, vec!["- born: 1990-01-01", "- x: @joe"]);
        assert!(section(body, "Missing").is_none());
    }

    #[test]
    fn sections_at_any_level_skip_fences() {
        let body = "# Ines\n\n## Summary\n```\n## Not a heading\n```\n### Coffee order\nflat white\n## Next\n";
        let (_, summary) = section(body, "Summary").unwrap();
        assert_eq!(summary.len(), 5, "fence and the ### subsection stay inside");
        let (head, coffee) = section(body, "Coffee order").unwrap();
        assert_eq!(head, "### Coffee order");
        assert_eq!(coffee, vec![(7, "flat white")]);
        assert!(section(body, "Not a heading").is_none());
        assert_eq!(
            first_heading("```\n# Code\n```\n# Real\n").as_deref(),
            Some("Real")
        );
    }
    #[test]
    fn counts_words() {
        assert_eq!(
            word_count("# Title\n\none two three\n```\nskip me\n```\nfour\n"),
            4
        );
    }
}
