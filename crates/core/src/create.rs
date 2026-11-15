//! `owo new`: creates an identity folder from the embedded templates.

use crate::identity::{Identity, LIFE_CATEGORIES};
use crate::write::Writes;
use crate::{index, state, templates};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub struct Created {
    pub slug: String,
    pub dir: std::path::PathBuf,
    pub files: Vec<String>,
}

/// Creates `<root>/<slug>`; error text names the folder under `shown`, the root as the request
/// gave it. A path already there, even a broken symlink, is an error and is left alone. On a
/// later error, the folder this call created is removed. A dry run makes no folder.
pub fn new_identity(root: &Path, shown: &Path, name: &str, slug: &str) -> Result<Created> {
    if !crate::slug::is_slug(slug) {
        bail!("invalid slug {slug:?}: use lowercase letters, digits and single hyphens");
    }
    let dir = root.join(slug);
    let shown_dir = shown.join(slug);
    let dry = crate::write::dry();
    if dry && fs::symlink_metadata(&dir).is_ok() {
        bail!("already exists: {}", shown_dir.display())
    }
    if !dry {
        fs::create_dir_all(root).with_context(|| format!("creating {}", shown.display()))?;
        match fs::create_dir(&dir) {
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                bail!("already exists: {}", shown_dir.display())
            }
            other => other.with_context(|| format!("creating {}", shown_dir.display()))?,
        }
    }
    let files = fill(root, &shown_dir, name, slug).inspect_err(|_| {
        if !dry {
            let _ = fs::remove_dir_all(&dir);
        }
    })?;
    Ok(Created {
        slug: slug.to_string(),
        dir,
        files,
    })
}

/// Writes the files of a new identity into `<root>/<slug>`, shown as `shown`.
fn fill(root: &Path, shown: &Path, name: &str, slug: &str) -> Result<Vec<String>> {
    let dir = root.join(slug);
    let mut values = BTreeMap::new();
    values.insert("name", name.to_string());
    values.insert("slug", slug.to_string());
    values.insert("today", crate::state::today());

    let folders = LIFE_CATEGORIES
        .iter()
        .map(|(category, _)| format!("life/{category}"))
        .chain(["work/projects".to_string()]);
    let writes = folders.fold(Writes::new(), |w, rel| {
        w.dir(dir.join(&rel), shown.join(&rel))
    });
    templates::IDENTITY_FILES
        .iter()
        .fold(writes, |w, (rel, builtin)| {
            let text = templates::fill(&templates::get(root, rel, builtin), &values);
            w.file(dir.join(rel), shown.join(rel), text)
        })
        .commit()?;
    let mut files: Vec<String> = templates::IDENTITY_FILES
        .iter()
        .map(|(rel, _)| rel.to_string())
        .collect();

    let identity = Identity::load(root, slug)?;
    files.extend(index::write_as(&identity, shown)?);
    let states = state::initial(&identity, &state::now());
    states
        .iter()
        .try_fold(Writes::new(), |w, (rel, value)| {
            w.json(dir.join(rel), shown.join(rel), value)
        })?
        .commit()?;
    files.extend(states.into_iter().map(|(rel, _)| rel));
    files.sort();
    Ok(files)
}
