//! `owo new`: creates an identity folder from the embedded templates.

use crate::identity::{Identity, LIFE_CATEGORIES};
use crate::{index, state, templates};
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub struct Created {
    pub slug: String,
    pub dir: std::path::PathBuf,
    pub files: Vec<String>,
}

/// Creates `<root>/<slug>`; error text names the folder under `shown`, the root as the request
/// gave it. On error after the folder check, the folder is removed.
pub fn new_identity(root: &Path, shown: &Path, name: &str, slug: &str) -> Result<Created> {
    if !crate::slug::is_slug(slug) {
        bail!("invalid slug {slug:?}: use lowercase letters, digits and single hyphens");
    }
    let dir = root.join(slug);
    let shown = shown.join(slug);
    if dir.exists() {
        bail!("already exists: {}", shown.display());
    }
    let files = fill(root, &shown, name, slug).inspect_err(|_| {
        let _ = fs::remove_dir_all(&dir);
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

    let mut files = vec![];
    for (rel, builtin) in templates::IDENTITY_FILES {
        let text = templates::fill(&templates::get(root, rel, builtin), &values);
        crate::identity::write_file_as(&dir.join(rel), &shown.join(rel), text)?;
        files.push(rel.to_string());
    }
    for (category, _) in LIFE_CATEGORIES {
        fs::create_dir_all(dir.join("life").join(category))?;
    }
    fs::create_dir_all(dir.join("work/projects"))?;

    let identity = Identity::load(root, slug)?;
    files.extend(index::write_as(&identity, shown)?);
    for (rel, value) in state::initial(&identity, &state::now()) {
        crate::identity::write_json_as(&dir.join(&rel), &shown.join(&rel), &value)?;
        files.push(rel);
    }
    files.sort();
    Ok(files)
}
