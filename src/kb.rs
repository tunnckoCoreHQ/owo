//! Project knowledge bases: where a project's KB is, the pointer `owo context` prints,
//! and `[[kb:<project>:<path>#<Section>]]` links for `owo show` and lint.

use crate::identity::Identity;
use crate::markdown;
use serde::Serialize;
use std::path::{Component, Path, PathBuf};

/// KB path for a project: project.md `kb`, relative to the identity folder (`~/` is the home folder).
pub fn root(id: &Identity, project: &str) -> Option<PathBuf> {
    let raw = id
        .project(project)
        .and_then(|e| e.field_str("kb"))
        .filter(|s| !s.is_empty())?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let p = crate::config::expand_home(&raw, home.as_deref());
    Some(if p.is_absolute() { p } else { id.dir.join(p) })
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[derive(Debug, Serialize)]
pub struct Kb {
    pub path: String,
    pub exists: bool,
    pub entry: Option<String>,
}

/// The KB pointer for `owo context`, and a warning when the KB is missing or has no path.
pub fn pointer(id: &Identity, p: &str) -> (Option<Kb>, Option<String>) {
    let Some(path) = root(id, p) else {
        return (
            None,
            Some(format!(
                "project {p} has no KB path (set kb in work/projects/{p}/project.md)"
            )),
        );
    };
    let path = normalize(&path);
    let exists = path.exists();
    let entry = if path.is_file() {
        Some(path.display().to_string())
    } else {
        ["lat.md", "README.md", "index.md"]
            .iter()
            .map(|f| path.join(f))
            .find(|f| f.is_file())
            .map(|f| f.display().to_string())
    };
    let warning = (!exists).then(|| format!("project KB not found at {}", path.display()));
    let kb = Kb {
        path: path.display().to_string(),
        exists,
        entry,
    };
    (Some(kb), warning)
}

/// A `[[kb:<project>:<path>#<Section>]]` link resolved against the project KB.
#[derive(Debug)]
pub struct Link {
    pub root_exists: bool,
    pub file: Option<String>,
    pub file_exists: bool,
    pub section: Option<String>,
    pub section_exists: bool,
}

/// Parses and resolves a kb link target (without brackets). None when it is not a kb link.
pub fn link(id: &Identity, target: &str) -> Option<Result<Link, String>> {
    let rest = target.strip_prefix("kb:")?;
    let Some((project, path)) = rest.split_once(':') else {
        return Some(Err(format!(
            "[[{target}]] must be kb:<project>:<path>[#<Section>]"
        )));
    };
    let (path, section) = match path.split_once('#') {
        Some((p, s)) => (p, Some(s.to_string())),
        None => (path, None),
    };
    if Path::new(path).components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Some(Err(format!(
            "[[{target}]]: the path must stay inside the KB (no `..` or absolute path)"
        )));
    }
    if id.project(project).is_none() {
        return Some(Err(format!("[[{target}]]: no project {project:?}")));
    }
    let mut out = Link {
        root_exists: false,
        file: None,
        file_exists: false,
        section,
        section_exists: false,
    };
    let Some(kb) = root(id, project) else {
        return Some(Ok(out));
    };
    let base = if kb.extension().is_some_and(|x| x == "md") {
        kb.parent().map(Path::to_path_buf).unwrap_or(kb.clone())
    } else {
        kb.clone()
    };
    out.root_exists = base.is_dir();
    let mut file = base.join(path);
    if !file.is_file() && file.extension().is_none() {
        file.set_extension("md");
    }
    out.file_exists = file.is_file();
    if out.file_exists {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        out.section_exists = match &out.section {
            None => true,
            Some(s) => markdown::section(&text, s).is_some(),
        };
    }
    out.file = Some(file.display().to_string());
    Some(Ok(out))
}

/// The KB file a kb link points at, as (path, text): the requested section, else the link's
/// section, else the whole file. None when the target is not a kb link.
pub fn show(
    id: &Identity,
    target: &str,
    section: Option<&str>,
) -> Option<anyhow::Result<(String, String)>> {
    let link = link(id, target)?;
    Some(link.map_err(anyhow::Error::msg).and_then(|k| {
        let file = k.file.clone().filter(|_| k.file_exists).ok_or_else(|| {
            anyhow::anyhow!(
                "KB file not found: {}",
                k.file.unwrap_or_else(|| "(no KB path)".into())
            )
        })?;
        let text = std::fs::read_to_string(&file)?;
        let body = match section.or(k.section.as_deref()) {
            None => text,
            Some(s) => crate::find::section_text(&text, s)
                .ok_or_else(|| anyhow::anyhow!("no section {s:?} in {file}"))?,
        };
        Ok((file, body))
    }))
}
