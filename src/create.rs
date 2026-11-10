//! `owo new`: creates an identity folder from the embedded templates.

use crate::identity::{Identity, LIFE_CATEGORIES};
use crate::{index, state, templates};
use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub struct Created {
    pub slug: String,
    pub dir: std::path::PathBuf,
    pub files: Vec<String>,
}

pub fn new_identity(root: &Path, name: &str, slug: &str) -> Result<Created> {
    if !crate::slug::is_slug(slug) {
        bail!("invalid slug {slug:?}: use lowercase letters, digits and single hyphens");
    }
    let dir = root.join(slug);
    if dir.exists() {
        bail!("already exists: {}", dir.display());
    }
    let mut values = BTreeMap::new();
    values.insert("name", name.to_string());
    values.insert("slug", slug.to_string());
    values.insert("today", chrono::Utc::now().format("%Y-%m-%d").to_string());

    let mut files = vec![];
    let write = |rel: &str, content: &str, files: &mut Vec<String>| -> Result<()> {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().expect("file has a parent"))
            .with_context(|| format!("creating {}", path.display()))?;
        fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
        files.push(rel.to_string());
        Ok(())
    };

    for (rel, builtin) in templates::IDENTITY_FILES {
        let text = templates::get(root, rel, builtin);
        write(rel, &templates::fill(&text, &values), &mut files)?;
    }
    for (category, _) in LIFE_CATEGORIES {
        fs::create_dir_all(dir.join("life").join(category))?;
    }
    fs::create_dir_all(dir.join("work/projects"))?;

    let identity = Identity::load(root, slug)?;
    files.extend(index::write(&identity)?);
    for (rel, value) in state::initial(&identity, &state::now()) {
        crate::identity::write_json(&dir.join(&rel), &value)?;
        files.push(rel);
    }
    files.sort();
    Ok(Created {
        slug: slug.to_string(),
        dir,
        files,
    })
}
