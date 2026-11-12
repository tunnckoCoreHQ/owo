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
            title: e.label(),
            kind: e.kind.name().to_string(),
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
    let matches_kind = |i: &usize| kind.is_none_or(|k| id.entries[*i].kind.name() == k);
    let hits: Vec<usize> = id
        .resolve_link(name)
        .into_iter()
        .filter(matches_kind)
        .collect();
    match hits.len() {
        1 => Resolution::Found(hits[0]),
        0 => {
            let needle = name.trim().to_lowercase();
            let mut scored: Vec<(usize, usize)> = id
                .keys
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

/// The error for a name that does not resolve to one entry, without the candidate list.
pub fn unresolved(name: &str) -> String {
    format!("{name:?} does not resolve to one entry")
}

/// The one entry a name resolves to; the error lists up to five candidates.
pub fn one(id: &Identity, name: &str) -> anyhow::Result<usize> {
    match resolve(id, name, None) {
        Resolution::Found(i) => Ok(i),
        Resolution::Ambiguous(v) | Resolution::Missing(v) => {
            let c: Vec<&str> = v.iter().map(|i| id.entries[*i].rel.as_str()).collect();
            let candidates = if c.is_empty() {
                String::new()
            } else {
                format!("; candidates: {}", c.join(", "))
            };
            anyhow::bail!("{}{candidates}", unresolved(name))
        }
    }
}

/// One section of `body` with its heading line (any heading level, case-insensitive);
/// the error names `path`.
pub fn section_text(body: &str, name: &str, path: &str) -> anyhow::Result<String> {
    let (head, text) = markdown::section_text(body, name)
        .ok_or_else(|| anyhow::anyhow!("no section {name:?} in {path}"))?;
    Ok(format!("{head}\n{}\n", text.trim_end()))
}

/// What `owo show` prints for a name, as (path, text): a `kb:` link's file, else an entry.
/// Either is the whole file or one section.
pub fn show(id: &Identity, name: &str, section: Option<&str>) -> anyhow::Result<(String, String)> {
    let target = name.trim_start_matches("[[").trim_end_matches("]]");
    if let Some(shown) = crate::kb::show(id, target, section) {
        return shown;
    }
    let e = &id.entries[one(id, name)?];
    let text = match section {
        None => e.text.clone(),
        Some(s) => section_text(e.body(), s, &e.rel)?,
    };
    Ok((e.rel.clone(), text))
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
    let mut out = vec![];
    for (src, e) in id.entries.iter().enumerate() {
        if src == idx || e.is_generated() {
            continue;
        }
        for (line, link) in file_links(e) {
            if id.resolve_link(&link.target).contains(&idx) {
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
    let e = &id.entries[idx];
    file_links(e)
        .into_iter()
        .map(|(line, link)| {
            let path = id
                .resolve_link(&link.target)
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

pub fn render_links(back: &[LinkRef], fwd: &[LinkRef]) -> String {
    let mut out = String::new();
    for l in back {
        out.push_str(&format!("in  {}:{}  {}\n", l.path, l.line, l.text));
    }
    for l in fwd {
        let to = if l.path.is_empty() {
            "(broken)"
        } else {
            &l.path
        };
        out.push_str(&format!(
            "out [[{}]] -> {to}  (line {})\n",
            l.target, l.line
        ));
    }
    out
}

/// Every wiki-link in a file with its 1-based line, frontmatter included,
/// leaving out `kb:` links and empty targets.
pub fn file_links(e: &Entry) -> Vec<(usize, markdown::Link)> {
    let mut out = raw_links(e);
    out.retain(|(_, l)| !l.target.is_empty() && !l.target.starts_with("kb:"));
    out
}

/// Every wiki-link in a file with its 1-based line, frontmatter included.
pub fn raw_links(e: &Entry) -> Vec<(usize, markdown::Link)> {
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
    let mut inbound = vec![0usize; id.entries.len()];
    for (src, e) in id
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_generated())
    {
        for (_, link) in file_links(e) {
            for hit in id.resolve_link(&link.target) {
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
