//! `owo resolve`, `owo show`, `owo links`: read-side lookups over one identity.

use crate::identity::{Entry, Identity, Kind};
use crate::markdown;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Hit {
    pub path: String,
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub summary: String,
}

impl Hit {
    pub fn of(e: &Entry) -> Self {
        Self {
            path: e.rel.clone(),
            title: e.title.clone().unwrap_or_else(|| e.stem_key()),
            kind: kind_name(e.kind).to_string(),
            summary: e
                .field_str("summary")
                .unwrap_or_default()
                .trim()
                .to_string(),
        }
    }

    pub fn render(&self) -> String {
        if self.summary.is_empty() {
            format!("{}  {}", self.path, self.title)
        } else {
            format!("{}  {}: {}", self.path, self.title, self.summary)
        }
    }
}

pub fn kind_name(kind: Kind) -> &'static str {
    kind.type_name().unwrap_or(match kind {
        Kind::Soul => "soul",
        Kind::Example => "example",
        Kind::WorkReadme => "work-readme",
        Kind::Generated => "generated",
        _ => "unknown",
    })
}

pub enum Resolution {
    Found(usize),
    Ambiguous(Vec<usize>),
    Missing(Vec<usize>),
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Resolves a name the way links do, optionally limited to one entry type.
pub fn resolve(id: &Identity, name: &str, kind: Option<&str>) -> Resolution {
    let keys = id.link_keys();
    let matches_kind = |i: &usize| kind.is_none_or(|k| kind_name(id.entries[*i].kind) == k);
    let hits: Vec<usize> = id
        .resolve_link(&keys, name)
        .into_iter()
        .filter(matches_kind)
        .collect();
    match hits.len() {
        1 => Resolution::Found(hits[0]),
        0 => {
            let needle = name.trim().to_lowercase();
            let mut scored: Vec<(usize, usize)> = keys
                .iter()
                .flat_map(|(k, idx)| {
                    let d = if k.contains(&needle) || needle.contains(k.as_str()) {
                        0
                    } else {
                        levenshtein(&needle, k)
                    };
                    idx.iter().map(move |i| (d, *i))
                })
                .filter(|(_, i)| matches_kind(i))
                .collect();
            scored.sort();
            let mut out = vec![];
            for (d, i) in scored {
                if d <= needle.len().max(4) / 2 && !out.contains(&i) {
                    out.push(i);
                }
                if out.len() == 5 {
                    break;
                }
            }
            Resolution::Missing(out)
        }
        _ => Resolution::Ambiguous(hits),
    }
}

/// Text of one `## section` (case-insensitive), or the whole file.
pub fn show(e: &Entry, section: Option<&str>) -> Option<String> {
    match section {
        None => Some(e.text.clone()),
        Some(name) => {
            let lines = markdown::section(e.body(), name)?;
            let body: Vec<&str> = lines.iter().map(|(_, l)| *l).collect();
            Some(format!("## {name}\n{}\n", body.join("\n").trim_end()))
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LinkRef {
    pub path: String,
    pub line: usize,
    pub text: String,
    pub target: String,
}

/// Links that point at entry `idx` (backlinks), from authored files.
pub fn backlinks(id: &Identity, idx: usize) -> Vec<LinkRef> {
    let keys = id.link_keys();
    let mut out = vec![];
    for (src, e) in id.entries.iter().enumerate() {
        if src == idx || e.is_generated() {
            continue;
        }
        for (line, link) in file_links(e) {
            if id.resolve_link(&keys, &link.target).contains(&idx) {
                out.push(LinkRef {
                    path: e.rel.clone(),
                    line,
                    text: line_text(e, line),
                    target: link.target,
                });
            }
        }
    }
    out
}

/// Links going out of entry `idx`, with the path they resolve to (empty when broken).
pub fn outlinks(id: &Identity, idx: usize) -> Vec<LinkRef> {
    let keys = id.link_keys();
    let e = &id.entries[idx];
    file_links(e)
        .into_iter()
        .map(|(line, link)| {
            let path = id
                .resolve_link(&keys, &link.target)
                .first()
                .map(|i| id.entries[*i].rel.clone())
                .unwrap_or_default();
            LinkRef {
                path,
                line,
                text: line_text(e, line),
                target: link.target,
            }
        })
        .collect()
}

/// Every wiki-link in a file with its 1-based line, frontmatter included.
pub fn file_links(e: &Entry) -> Vec<(usize, markdown::Link)> {
    let split = markdown::split(&e.text);
    let mut out = vec![];
    if let Some(fm) = split.frontmatter {
        out.extend(markdown::links(fm).into_iter().map(|l| (l.line + 2, l)));
    }
    out.extend(
        markdown::links(split.body)
            .into_iter()
            .map(|l| (l.line + split.body_line, l)),
    );
    out.retain(|(_, l)| !l.target.is_empty() && !l.target.starts_with("kb:"));
    out
}

fn line_text(e: &Entry, line: usize) -> String {
    e.text
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Entries that should be linked from somewhere but are not.
pub fn orphans(id: &Identity) -> Vec<usize> {
    let keys = id.link_keys();
    let mut inbound = vec![0usize; id.entries.len()];
    for (src, e) in id
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_generated())
    {
        for (_, link) in file_links(e) {
            for hit in id.resolve_link(&keys, &link.target) {
                if hit != src {
                    inbound[hit] += 1;
                }
            }
        }
    }
    const WATCHED: [Kind; 12] = [
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
        Kind::Project,
    ];
    (0..id.entries.len())
        .filter(|i| WATCHED.contains(&id.entries[*i].kind) && inbound[*i] == 0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::levenshtein;

    #[test]
    fn edit_distance() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }
}
