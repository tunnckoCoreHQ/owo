use crate::identity::{life_categories, Identity};
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

pub fn new_identity(
    identities_dir: &Path,
    shown: &Path,
    templates_dir: Option<&Path>,
    name: &str,
    slug: &str,
) -> Result<Created> {
    if !crate::slug::is_slug(slug) {
        bail!("invalid slug {slug:?}: use lowercase letters, digits and single hyphens");
    }
    let dir = identities_dir.join(slug);
    let shown_dir = shown.join(slug);
    let dry = crate::write::dry();
    let taken = if dry {
        fs::symlink_metadata(&dir).is_ok()
    } else {
        fs::create_dir_all(identities_dir)
            .with_context(|| format!("creating {}", shown.display()))?;
        match fs::create_dir(&dir) {
            Err(e) if e.kind() == ErrorKind::AlreadyExists => true,
            other => {
                other.with_context(|| format!("creating {}", shown_dir.display()))?;
                false
            }
        }
    };
    if taken {
        bail!("already exists: {}", shown_dir.display())
    }
    let files = fill(identities_dir, &shown_dir, templates_dir, name, slug).inspect_err(|_| {
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

fn fill(
    identities_dir: &Path,
    shown: &Path,
    templates_dir: Option<&Path>,
    name: &str,
    slug: &str,
) -> Result<Vec<String>> {
    let dir = identities_dir.join(slug);
    let mut values = BTreeMap::new();
    values.insert("name", name.to_string());
    values.insert("slug", slug.to_string());
    values.insert("today", crate::state::today());

    let folders = life_categories()
        .map(|(category, _)| format!("life/{category}"))
        .chain(["work/projects".to_string()]);
    let writes = folders.fold(Writes::default(), |w, rel| {
        w.dir(dir.join(&rel), shown.join(&rel))
    });
    templates::IDENTITY_FILES
        .iter()
        .fold(writes, |w, (rel, builtin)| {
            let text = templates::fill(&templates::get(templates_dir, rel, builtin), &values);
            w.file(dir.join(rel), shown.join(rel), text)
        })
        .commit()?;
    let mut files: Vec<String> = templates::IDENTITY_FILES
        .iter()
        .map(|(rel, _)| rel.to_string())
        .collect();

    let identity = Identity::load(identities_dir, slug)?;
    files.extend(index::write_as(&identity, shown)?);
    let states = state::initial(&identity, &state::now());
    states
        .iter()
        .try_fold(Writes::default(), |w, (rel, value)| {
            w.json(dir.join(rel), shown.join(rel), value)
        })?
        .commit()?;
    files.extend(states.into_iter().map(|(rel, _)| rel));
    files.sort();
    Ok(files)
}
