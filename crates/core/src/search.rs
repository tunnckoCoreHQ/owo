use crate::identity::{Entry, Identity};
use crate::Named;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const K1: f64 = 1.2;
const B: f64 = 0.75;

pub fn tokens(text: &str) -> Vec<String> {
    deunicode::deunicode(text)
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 1)
        .map(str::to_string)
        .collect()
}

fn doc_tokens(e: &Entry) -> Vec<String> {
    let head = tokens(&format!("{} {}", e.label(), e.aliases.join(" ")));
    let summary = e.field_str("summary").map_or_else(Vec::new, |s| tokens(&s));
    std::iter::repeat_n(&head, 3)
        .chain(std::iter::repeat_n(&summary, 2))
        .flatten()
        .cloned()
        .chain(tokens(e.body()))
        .collect()
}

#[derive(Debug, Serialize)]
pub struct Hit {
    pub path: String,
    pub title: String,
    pub score: f64,
    pub line: Option<usize>,
    pub snippet: Option<String>,
    #[serde(skip)]
    exact: bool,
}

named! {
    pub enum Scope {
        Soul = "soul",
        Life = "life",
        Work = "work",
    }
}

use crate::SearchRequest as Request;

impl Request {
    fn keep(&self, e: &Entry) -> bool {
        let since = self.since.map(|d| d.to_string());
        self.scope
            .is_none_or(|s| e.rel.starts_with(&format!("{}/", s.name())))
            && (self.types.is_empty() || self.types.contains(&e.kind))
            && self
                .project
                .as_deref()
                .is_none_or(|p| e.rel.starts_with(&format!("work/projects/{p}/")))
            && since.is_none_or(|s| {
                e.field_str("date")
                    .or_else(|| e.field_str("updated"))
                    .is_some_and(|d| d >= s)
            })
    }
}

pub fn search(id: &Identity, filter: &Request) -> Vec<Hit> {
    let q = tokens(&filter.query);
    let docs: Vec<(&Entry, Vec<String>)> = id
        .entries
        .iter()
        .filter(|e| !e.is_generated() && filter.keep(e))
        .map(|e| (e, doc_tokens(e)))
        .collect();
    if docs.is_empty() || q.is_empty() {
        return vec![];
    }
    let n = docs.len() as f64;
    let avg = docs.iter().map(|(_, t)| t.len()).sum::<usize>() as f64 / n;
    let mut df: HashMap<&str, usize> = HashMap::new();
    for (_, toks) in &docs {
        for t in toks.iter().map(String::as_str).collect::<HashSet<_>>() {
            *df.entry(t).or_default() += 1;
        }
    }
    let mut hits = vec![];
    for (e, toks) in &docs {
        let len = toks.len() as f64;
        let mut score = 0.0;
        for term in &q {
            let tf = toks.iter().filter(|t| *t == term).count() as f64;
            if tf == 0.0 {
                continue;
            }
            let d = *df.get(term.as_str()).unwrap_or(&0) as f64;
            let idf = ((n - d + 0.5) / (d + 0.5) + 1.0).ln();
            score += idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * len / avg));
        }
        if score > 0.0 {
            let names: Vec<&String> = e.title.iter().chain(&e.aliases).collect();
            let body_line = crate::markdown::split(&e.text).body_line;
            let best_in = |in_body: bool| {
                e.text
                    .lines()
                    .enumerate()
                    .filter(|(i, l)| (i + 1 >= body_line) == in_body && !l.starts_with("# "))
                    .map(|(i, l)| (tokens(l).iter().filter(|t| q.contains(t)).count(), i, l))
                    .filter(|(c, _, _)| *c > 0)
                    .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
            };
            let best = best_in(true).or_else(|| best_in(false));
            hits.push(Hit {
                path: e.rel.clone(),
                title: e.label(),
                score: (score * 1000.0).round() / 1000.0,
                line: best.map(|(_, i, _)| i + 1),
                snippet: best.map(|(_, _, l)| l.trim().chars().take(160).collect()),
                exact: names.iter().any(|n| tokens(n) == q),
            });
        }
    }
    hits.sort_by(|a, b| {
        (b.exact.cmp(&a.exact))
            .then(b.score.total_cmp(&a.score))
            .then(a.path.cmp(&b.path))
    });
    hits.truncate(filter.limit);
    hits
}

#[cfg(test)]
mod tests {
    #[test]
    fn tokens_fold_and_split() {
        assert_eq!(super::tokens("Café-Sail, a B2"), vec!["cafe", "sail", "b2"]);
    }
}
