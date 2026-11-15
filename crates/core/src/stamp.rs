use crate::identity::{Identity, Kind};
use crate::state::{self, hash, scope_of, SCOPES};
use crate::write::Writes;
use anyhow::{bail, Result};
use regex::Regex;
use serde_json::{json, Map, Value};

#[derive(Default, serde::Serialize)]
pub struct Report {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

fn load(id: &Identity, scope: &str) -> Result<Value> {
    Ok(id
        .read_object(&format!("{scope}/state.json"))?
        .unwrap_or_else(|| json!({ "files": {} })))
}

fn files_mut(doc: &mut Value) -> &mut Map<String, Value> {
    if !doc.get("files").is_some_and(Value::is_object) {
        doc["files"] = json!({});
    }
    doc["files"].as_object_mut().expect("files is an object")
}

fn touch_updated(text: &str, today: &str) -> Option<String> {
    let split = crate::markdown::split(text);
    let fm = split.frontmatter?;
    let nl = if text.starts_with("---\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let re = Regex::new(r"(?m)^updated:[^\r\n]*").expect("valid regex");
    let line = format!("updated: {today}");
    let new_fm = if re.is_match(fm) {
        re.replace(fm, line.as_str()).to_string()
    } else {
        format!("{fm}{line}{nl}")
    };
    (new_fm != fm).then(|| format!("---{nl}{new_fm}---{nl}{}", split.body))
}

pub fn run(id: &Identity, check: bool) -> Result<Report> {
    if let Some(e) = id.entries.iter().find(|e| e.lossy && !check) {
        bail!("{} is not valid UTF-8; fix it before stamping", e.rel);
    }
    let at = state::now();
    let today = state::local_today();
    let mut docs = SCOPES
        .iter()
        .map(|s| Ok((*s, load(id, s)?)))
        .collect::<Result<Vec<(&str, Value)>>>()?;
    let mut report = Report::default();
    let mut soul_changed = false;
    let mut seen: Vec<(String, String)> = vec![];
    let mut touched: Vec<(String, String)> = vec![];

    for e in state::tracked(id) {
        let Some((scope, rel)) = scope_of(&e.rel) else {
            continue;
        };
        seen.push((scope.to_string(), rel.to_string()));
        let doc = &mut docs
            .iter_mut()
            .find(|(s, _)| *s == scope)
            .expect("scope exists")
            .1;
        let recorded = files_mut(doc)
            .get(rel)
            .and_then(|r| r.get("hash"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let current = hash(&e.text);
        if recorded.as_deref() == Some(current.as_str()) {
            continue;
        }
        if recorded.is_none() {
            report.added.push(e.rel.clone());
        } else {
            report.changed.push(e.rel.clone());
        }
        soul_changed |= e.kind == Kind::Soul;
        if check {
            continue;
        }
        let mut text = e.text.clone();
        if recorded.is_some() && e.kind.has_frontmatter() {
            if let Some(new_text) = touch_updated(&text, &today) {
                touched.push((e.rel.clone(), new_text.clone()));
                text = new_text;
            }
        }
        let files = files_mut(doc);
        files.insert(
            rel.to_string(),
            json!({ "updated": at, "hash": hash(&text) }),
        );
    }

    for (scope, doc) in docs.iter_mut() {
        let gone: Vec<String> = files_mut(doc)
            .keys()
            .filter(|k| !seen.contains(&(scope.to_string(), (*k).clone())))
            .cloned()
            .collect();
        for k in gone {
            report.removed.push(format!("{scope}/{k}"));
            if !check {
                files_mut(doc).remove(&k);
            }
        }
    }
    if check || report.is_clean() {
        return Ok(report);
    }
    let soul = if soul_changed { bumped_soul(id)? } else { None };
    let writes = touched.into_iter().fold(Writes::new(), |w, (rel, text)| {
        w.file(id.dir.join(&rel), rel, text)
    });
    let writes = docs.iter().try_fold(writes, |w, (scope, doc)| {
        let path = id.dir.join(format!("{scope}/state.json"));
        w.json(&path, &path, doc)
    })?;
    let path = id.dir.join("soul/soul.json");
    match soul {
        Some(soul) => writes.json(&path, &path, &soul)?,
        None => writes,
    }
    .commit()?;
    Ok(report)
}

fn bumped_soul(id: &Identity) -> Result<Option<Value>> {
    let Some(mut soul) = id.read_object("soul/soul.json")? else {
        return Ok(None);
    };
    let mut parts = soul
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("0.0.0")
        .split('.')
        .map(|p| p.parse().unwrap_or(0));
    let (major, minor): (u64, u64) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    soul["version"] = json!(format!("{major}.{}.0", minor + 1));
    Ok(Some(soul))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touches_updated_line() {
        let t = "---\ntitle: A\nupdated: 2020-01-01\n---\n# A\n";
        assert_eq!(
            touch_updated(t, "2026-11-04").unwrap(),
            "---\ntitle: A\nupdated: 2026-11-04\n---\n# A\n"
        );
        assert!(touch_updated("---\nupdated: 2026-11-04\n---\n", "2026-11-04").is_none());
        let crlf = "---\r\ntitle: A\r\nupdated: 2020-01-01\r\n---\r\n# A\r\n";
        assert_eq!(
            touch_updated(crlf, "2026-11-04").unwrap(),
            "---\r\ntitle: A\r\nupdated: 2026-11-04\r\n---\r\n# A\r\n"
        );
    }
}
