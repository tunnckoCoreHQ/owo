use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = ".owo.toml";

#[derive(Debug, Default, Deserialize)]
pub struct FileConfig {
    pub root: Option<String>,
    pub identity: Option<String>,
    pub project: Option<String>,
    pub sync_every: Option<usize>,
    pub auto_ingest: Option<bool>,
    pub ingest_command: Option<Vec<String>>,
}

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
    pub sync_every: Option<usize>,
    pub auto_ingest: bool,
    pub ingest_command: Option<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct Inputs {
    pub select: crate::Select,
    pub env_root: Option<String>,
    pub env_identity: Option<String>,
    pub env_project: Option<String>,
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
    pub xdg_config: Option<PathBuf>,
    pub xdg_data: Option<PathBuf>,
}

impl Inputs {
    pub fn new(select: &crate::Select, env: &crate::Env) -> Self {
        Self {
            select: select.clone(),
            env_root: env.owo_root.clone(),
            env_identity: env.owo_identity.clone(),
            env_project: env.owo_project.clone(),
            cwd: env.cwd.clone(),
            home: env
                .home
                .clone()
                .filter(|h| h.to_str().is_some_and(|s| !s.is_empty())),
            xdg_config: env.xdg_config_home.clone(),
            xdg_data: env.xdg_data_home.clone(),
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
        let path = expand_home(value, self.home.as_deref());
        if path.is_absolute() {
            path
        } else {
            relative_to.join(path)
        }
    }
}

pub fn expand_home(value: &str, home: Option<&Path>) -> PathBuf {
    match (value.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(value),
    }
}

fn read_config(path: &Path) -> Result<FileConfig> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

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

    let (root, root_source) = if let Some(picked) = &inputs.select.root {
        (
            inputs.expand(&picked.to_string_lossy(), &inputs.cwd),
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
        return Err(crate::Fault::NoRoot.into());
    };

    let (identity, identity_source) = if let Some(picked) = &inputs.select.identity {
        (Some(picked.clone()), Some(Source::Flag))
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
        .select
        .project
        .clone()
        .or_else(|| inputs.env_project.clone())
        .or_else(|| project_cfg.as_ref().and_then(|(_, c)| c.project.clone()));
    let configs = [&project_cfg, &user_cfg];

    Ok(Resolved {
        root,
        root_source,
        identity,
        identity_source,
        project,
        sync_every: first(configs, |c| c.sync_every),
        auto_ingest: first(configs, |c| c.auto_ingest).unwrap_or(false),
        ingest_command: first(configs, |c| c.ingest_command.clone()),
    })
}

fn first<T>(
    configs: [&Option<(PathBuf, FileConfig)>; 2],
    field: impl Fn(&FileConfig) -> Option<T>,
) -> Option<T> {
    configs
        .into_iter()
        .find_map(|cfg| cfg.as_ref().and_then(|(_, c)| field(c)))
}

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

#[derive(Debug)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

pub fn need_identity(r: &Resolved) -> Result<String> {
    let slug = r
        .identity
        .clone()
        .ok_or_else(|| crate::Fault::NoIdentity(r.root.clone()))?;
    if !crate::slug::is_slug(&slug) {
        return Err(ConfigError(format!(
            "invalid identity {slug:?}: use lowercase letters, digits and single hyphens"
        ))
        .into());
    }
    if !r.root.join(&slug).is_dir() {
        return Err(crate::Fault::NoIdentityFolder(r.root.join(&slug)).into());
    }
    Ok(slug)
}

pub fn need_project<'a>(dir: &Path, project: Option<&'a str>) -> Result<Option<&'a str>> {
    let Some(p) = project else {
        return Ok(None);
    };
    if !crate::slug::is_slug(p) {
        return Err(ConfigError(format!(
            "invalid project {p:?}: use lowercase letters, digits and single hyphens"
        ))
        .into());
    }
    if !dir.join(format!("work/projects/{p}/project.md")).is_file() {
        return Err(crate::Fault::NoProject(p.to_string()).into());
    }
    Ok(Some(p))
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

        i.select.root = Some(PathBuf::from("/flag/root"));
        i.select.identity = Some("flag-id".into());
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
        i.select.root = Some(tmp.path().to_path_buf());
        let r = resolve(&i).unwrap();
        assert_eq!(r.identity.as_deref(), Some("thea"));
    }
}
