//! Root, identity and project resolution.
//!
//! Root order: `--root`, `OWO_ROOT`, `root` in the nearest `.owo.toml`, `root` in the
//! user config, then `$XDG_DATA_HOME/owo`. Identity and project follow the same chain.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = ".owo.toml";

/// Fields shared by `.owo.toml` and the user config.
#[derive(Debug, Default, Deserialize)]
pub struct FileConfig {
    pub root: Option<String>,
    pub identity: Option<String>,
    pub project: Option<String>,
    pub kb: Option<String>,
}

/// Where a resolved value came from, reported by `owo config`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Flag,
    Env,
    ProjectFile(PathBuf),
    UserConfig(PathBuf),
    Default,
    OnlyIdentity,
}

#[derive(Debug, serde::Serialize)]
pub struct Resolved {
    pub root: PathBuf,
    pub root_source: Source,
    pub identity: Option<String>,
    pub identity_source: Option<Source>,
    pub project: Option<String>,
    pub kb: Option<String>,
}

/// Inputs to resolution, so tests can run without touching the real environment.
#[derive(Debug, Default)]
pub struct Inputs {
    pub root_flag: Option<PathBuf>,
    pub identity_flag: Option<String>,
    pub project_flag: Option<String>,
    pub env_root: Option<String>,
    pub env_identity: Option<String>,
    pub env_project: Option<String>,
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
    pub xdg_config: Option<PathBuf>,
    pub xdg_data: Option<PathBuf>,
}

impl Inputs {
    pub fn from_env(
        root_flag: Option<PathBuf>,
        identity_flag: Option<String>,
        project_flag: Option<String>,
    ) -> Self {
        let non_empty = |key: &str| env::var(key).ok().filter(|v| !v.is_empty());
        Self {
            root_flag,
            identity_flag,
            project_flag,
            env_root: non_empty("OWO_ROOT"),
            env_identity: non_empty("OWO_IDENTITY"),
            env_project: non_empty("OWO_PROJECT"),
            cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            home: non_empty("HOME").map(PathBuf::from),
            xdg_config: non_empty("XDG_CONFIG_HOME").map(PathBuf::from),
            xdg_data: non_empty("XDG_DATA_HOME").map(PathBuf::from),
        }
    }

    fn user_config_path(&self) -> Option<PathBuf> {
        let base = self
            .xdg_config
            .clone()
            .or_else(|| self.home.as_ref().map(|h| h.join(".config")))?;
        Some(base.join("owo").join("config.toml"))
    }

    fn default_root(&self) -> Option<PathBuf> {
        let base = self
            .xdg_data
            .clone()
            .or_else(|| self.home.as_ref().map(|h| h.join(".local/share")))?;
        Some(base.join("owo"))
    }

    fn expand(&self, value: &str, relative_to: &Path) -> PathBuf {
        if let Some(rest) = value.strip_prefix("~/") {
            if let Some(home) = &self.home {
                return home.join(rest);
            }
        }
        let path = PathBuf::from(value);
        if path.is_absolute() {
            path
        } else {
            relative_to.join(path)
        }
    }
}

fn read_config(path: &Path) -> Result<FileConfig> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Finds the nearest `.owo.toml` walking up from `start`.
pub fn find_project_file(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(PROJECT_FILE))
        .find(|p| p.is_file())
}

pub fn resolve(inputs: &Inputs) -> Result<Resolved> {
    let project_file = find_project_file(&inputs.cwd);
    let project_cfg = match &project_file {
        Some(p) => Some((p.clone(), read_config(p)?)),
        None => None,
    };
    let user_path = inputs.user_config_path().filter(|p| p.is_file());
    let user_cfg = match &user_path {
        Some(p) => Some((p.clone(), read_config(p)?)),
        None => None,
    };

    let (root, root_source) = if let Some(flag) = &inputs.root_flag {
        (
            inputs.expand(&flag.to_string_lossy(), &inputs.cwd),
            Source::Flag,
        )
    } else if let Some(value) = &inputs.env_root {
        (inputs.expand(value, &inputs.cwd), Source::Env)
    } else if let Some((path, value)) = project_cfg
        .as_ref()
        .and_then(|(p, c)| c.root.as_ref().map(|r| (p, r)))
    {
        (
            inputs.expand(value, path.parent().unwrap_or(Path::new("."))),
            Source::ProjectFile(path.clone()),
        )
    } else if let Some((path, value)) = user_cfg
        .as_ref()
        .and_then(|(p, c)| c.root.as_ref().map(|r| (p, r)))
    {
        (
            inputs.expand(value, path.parent().unwrap_or(Path::new("."))),
            Source::UserConfig(path.clone()),
        )
    } else if let Some(root) = inputs.default_root() {
        (root, Source::Default)
    } else {
        bail!("cannot resolve a root: pass --root or set OWO_ROOT");
    };

    let (identity, identity_source) = if let Some(flag) = &inputs.identity_flag {
        (Some(flag.clone()), Some(Source::Flag))
    } else if let Some(value) = &inputs.env_identity {
        (Some(value.clone()), Some(Source::Env))
    } else if let Some((path, value)) = project_cfg
        .as_ref()
        .and_then(|(p, c)| c.identity.as_ref().map(|v| (p, v)))
    {
        (Some(value.clone()), Some(Source::ProjectFile(path.clone())))
    } else if let Some((path, value)) = user_cfg
        .as_ref()
        .and_then(|(p, c)| c.identity.as_ref().map(|v| (p, v)))
    {
        (Some(value.clone()), Some(Source::UserConfig(path.clone())))
    } else {
        match only_identity(&root) {
            Some(slug) => (Some(slug), Some(Source::OnlyIdentity)),
            None => (None, None),
        }
    };

    let project = inputs
        .project_flag
        .clone()
        .or_else(|| inputs.env_project.clone())
        .or_else(|| project_cfg.as_ref().and_then(|(_, c)| c.project.clone()));

    let kb = project_cfg.as_ref().and_then(|(file, c)| {
        let raw = PathBuf::from(c.kb.as_ref()?);
        let base = file.parent().unwrap_or(Path::new("."));
        Some(
            if raw.is_absolute() {
                raw
            } else {
                base.join(raw)
            }
            .display()
            .to_string(),
        )
    });
    Ok(Resolved {
        root,
        root_source,
        identity,
        identity_source,
        project,
        kb,
    })
}

/// Lists identity folders: direct children of the root that hold `identity.json`.
pub fn list_identities(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return vec![];
    };
    let mut slugs: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("identity.json").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    slugs.sort();
    slugs
}

fn only_identity(root: &Path) -> Option<String> {
    let slugs = list_identities(root);
    if slugs.len() == 1 {
        slugs.into_iter().next()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(dir: &Path) -> Inputs {
        Inputs {
            cwd: dir.to_path_buf(),
            home: Some(dir.join("home")),
            ..Default::default()
        }
    }

    #[test]
    fn default_root_uses_xdg_data_home_then_home() {
        let tmp = tempfile::tempdir().unwrap();
        let mut i = inputs(tmp.path());
        assert_eq!(
            resolve(&i).unwrap().root,
            tmp.path().join("home/.local/share/owo")
        );
        i.xdg_data = Some(tmp.path().join("data"));
        assert_eq!(resolve(&i).unwrap().root, tmp.path().join("data/owo"));
    }

    #[test]
    fn precedence_flag_env_project_user() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = tmp.path().join("home/.config/owo");
        fs::create_dir_all(&cfg).unwrap();
        fs::write(
            cfg.join("config.toml"),
            "root = \"~/ids\"\nidentity = \"user-pick\"\n",
        )
        .unwrap();
        let mut i = inputs(tmp.path());
        let r = resolve(&i).unwrap();
        assert_eq!(r.root, tmp.path().join("home/ids"));
        assert_eq!(r.identity.as_deref(), Some("user-pick"));

        let proj = tmp.path().join("proj/sub");
        fs::create_dir_all(&proj).unwrap();
        fs::write(
            tmp.path().join("proj/.owo.toml"),
            "root = \"ids\"\nidentity = \"thea\"\nproject = \"api\"\n",
        )
        .unwrap();
        i.cwd = proj;
        let r = resolve(&i).unwrap();
        assert_eq!(r.root, tmp.path().join("proj/ids"));
        assert_eq!(r.identity.as_deref(), Some("thea"));
        assert_eq!(r.project.as_deref(), Some("api"));

        i.env_root = Some("/env/root".into());
        i.env_identity = Some("env-id".into());
        let r = resolve(&i).unwrap();
        assert_eq!(r.root, PathBuf::from("/env/root"));
        assert_eq!(r.identity.as_deref(), Some("env-id"));

        i.root_flag = Some(PathBuf::from("/flag/root"));
        i.identity_flag = Some("flag-id".into());
        let r = resolve(&i).unwrap();
        assert_eq!(r.root, PathBuf::from("/flag/root"));
        assert_eq!(r.identity.as_deref(), Some("flag-id"));
    }

    #[test]
    fn single_identity_is_picked() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("thea")).unwrap();
        fs::write(tmp.path().join("thea/identity.json"), "{}").unwrap();
        let mut i = inputs(tmp.path());
        i.root_flag = Some(tmp.path().to_path_buf());
        let r = resolve(&i).unwrap();
        assert_eq!(r.identity.as_deref(), Some("thea"));
    }
}
