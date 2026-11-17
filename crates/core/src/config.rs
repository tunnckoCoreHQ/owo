use crate::{Env, Error, ErrorKind, Fault, Result, Select};
use anyhow::Context;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub identities_dir: Option<String>,
    pub templates_dir: Option<String>,
    pub identity: Option<String>,
    pub sync_every: Option<usize>,
    pub auto_ingest: Option<bool>,
    pub ingest_command: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    Flag,
    Env,
    GlobalConfig(PathBuf),
    Default,
    OnlyIdentity,
}

#[derive(Debug, serde::Serialize)]
pub struct Resolved {
    pub owo_dir: PathBuf,
    pub owo_dir_source: Origin,
    pub config: PathBuf,
    pub config_source: Origin,
    pub identities_dir: PathBuf,
    pub identities_dir_source: Origin,
    pub templates_dir: PathBuf,
    pub templates_dir_source: Origin,
    pub identity: Option<String>,
    pub identity_source: Option<Origin>,
    pub project: Option<String>,
    pub sync_every: Option<usize>,
    pub auto_ingest: bool,
    pub ingest_command: Option<Vec<String>>,
}

pub fn expand_home(value: &str, home: Option<&Path>) -> PathBuf {
    match (value.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(value),
    }
}

fn load(path: &Path) -> anyhow::Result<FileConfig> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn resolve(select: &Select, env: &Env) -> Result<Resolved> {
    let home = env
        .home
        .as_deref()
        .filter(|h| h.to_str().is_some_and(|s| !s.is_empty()));
    // A flag or a variable is a path from the working folder.
    let given = |flag: &Option<PathBuf>, var: &Option<String>| {
        let flag = flag.as_ref().map(|p| (env.cwd.join(p), Origin::Flag));
        flag.or_else(|| {
            var.as_ref()
                .map(|v| (env.cwd.join(expand_home(v, home)), Origin::Env))
        })
    };

    let (owo_dir, owo_dir_source) = match given(&select.owo_dir, &env.owo_dir) {
        Some(found) => found,
        None => (
            env.xdg_config_home
                .clone()
                .or_else(|| home.map(|h| h.join(".config")))
                .ok_or(Fault::NoOwoDir)?
                .join("owo"),
            Origin::Default,
        ),
    };
    let (config, config_source) = given(&select.config, &env.owo_config)
        .unwrap_or_else(|| (owo_dir.join(CONFIG_FILE), Origin::Default));
    let file = match config_source {
        Origin::Default if !config.is_file() => FileConfig::default(),
        _ => load(&config)?,
    };
    // A path in the config is a path from the folder of the config file.
    let base = config.parent().unwrap_or(Path::new("."));
    let dir = |found: Option<(PathBuf, Origin)>, key: &Option<String>, default: &str| {
        found
            .or_else(|| {
                let path = base.join(expand_home(key.as_ref()?, home));
                Some((path, Origin::GlobalConfig(config.clone())))
            })
            .unwrap_or_else(|| (owo_dir.join(default), Origin::Default))
    };
    let (identities_dir, identities_dir_source) = dir(
        given(&select.identities_dir, &env.owo_identities_dir),
        &file.identities_dir,
        "identities",
    );
    let (templates_dir, templates_dir_source) = dir(
        given(&None, &env.owo_templates_dir),
        &file.templates_dir,
        "templates",
    );

    let (identity, identity_source) = select
        .identity
        .clone()
        .map(|v| (v, Origin::Flag))
        .or_else(|| env.owo_identity.clone().map(|v| (v, Origin::Env)))
        .or_else(|| {
            let origin = Origin::GlobalConfig(config.clone());
            file.identity.clone().map(|v| (v, origin))
        })
        .or_else(|| only_identity(&identities_dir).map(|slug| (slug, Origin::OnlyIdentity)))
        .unzip();
    let project = select.project.clone().or_else(|| env.owo_project.clone());

    Ok(Resolved {
        owo_dir,
        owo_dir_source,
        config,
        config_source,
        identities_dir,
        identities_dir_source,
        templates_dir,
        templates_dir_source,
        identity,
        identity_source,
        project,
        sync_every: file.sync_every,
        auto_ingest: file.auto_ingest.unwrap_or(false),
        ingest_command: file.ingest_command,
    })
}

pub fn list_identities(identities_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(identities_dir) else {
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

fn only_identity(identities_dir: &Path) -> Option<String> {
    let mut slugs = list_identities(identities_dir);
    if slugs.len() == 1 {
        slugs.pop()
    } else {
        None
    }
}

fn check_slug(noun: &str, value: &str) -> Result<()> {
    if crate::slug::is_slug(value) {
        return Ok(());
    }
    Err(Error::new(
        ErrorKind::Config,
        format!("invalid {noun} {value:?}: use lowercase letters, digits and single hyphens"),
    ))
}

pub fn need_identity(r: &Resolved) -> Result<String> {
    let slug = r
        .identity
        .clone()
        .ok_or_else(|| Fault::NoIdentity(r.identities_dir.clone()))?;
    check_slug("identity", &slug)?;
    if !r.identities_dir.join(&slug).is_dir() {
        return Err(Fault::NoIdentityFolder(r.identities_dir.join(&slug)).into());
    }
    Ok(slug)
}

pub fn need_project<'a>(dir: &Path, project: Option<&'a str>) -> Result<Option<&'a str>> {
    let Some(p) = project else {
        return Ok(None);
    };
    check_slug("project", p)?;
    if !dir.join(format!("work/projects/{p}/README.md")).is_file() {
        return Err(Fault::NoProject(p.to_string()).into());
    }
    Ok(Some(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(dir: &Path) -> Env {
        Env {
            cwd: dir.to_path_buf(),
            home: Some(dir.join("home")),
            ..Default::default()
        }
    }

    #[test]
    fn default_owo_dir_uses_xdg_config_home_then_home() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = env(tmp.path());
        let r = resolve(&Select::default(), &e).unwrap();
        let owo_dir = tmp.path().join("home/.config/owo");
        assert_eq!(r.owo_dir, owo_dir);
        assert_eq!(r.config, owo_dir.join("config.toml"));
        assert_eq!(r.identities_dir, owo_dir.join("identities"));
        assert_eq!(r.templates_dir, owo_dir.join("templates"));
        e.xdg_config_home = Some(tmp.path().join("xdg"));
        let r = resolve(&Select::default(), &e).unwrap();
        assert_eq!(r.identities_dir, tmp.path().join("xdg/owo/identities"));
    }

    #[test]
    fn precedence_flag_env_config_default() {
        let tmp = tempfile::tempdir().unwrap();
        let owo_dir = tmp.path().join("home/.config/owo");
        fs::create_dir_all(&owo_dir).unwrap();
        fs::write(
            owo_dir.join(CONFIG_FILE),
            "identities_dir = \"~/ids\"\ntemplates_dir = \"tpl\"\nidentity = \"config-pick\"\nsync_every = 3\n",
        )
        .unwrap();
        let mut s = Select::default();
        let mut e = env(tmp.path());
        let r = resolve(&s, &e).unwrap();
        assert_eq!(r.identities_dir, tmp.path().join("home/ids"));
        assert_eq!(
            r.templates_dir,
            owo_dir.join("tpl"),
            "from the config folder"
        );
        assert_eq!(r.identity.as_deref(), Some("config-pick"));
        assert_eq!(r.sync_every, Some(3));

        e.owo_identities_dir = Some("env-ids".into());
        e.owo_templates_dir = Some("/env/tpl".into());
        e.owo_identity = Some("env-id".into());
        let r = resolve(&s, &e).unwrap();
        assert_eq!(r.identities_dir, tmp.path().join("env-ids"));
        assert_eq!(r.templates_dir, PathBuf::from("/env/tpl"));
        assert_eq!(r.identity.as_deref(), Some("env-id"));

        s.identities_dir = Some(PathBuf::from("/flag/ids"));
        s.identity = Some("flag-id".into());
        let r = resolve(&s, &e).unwrap();
        assert_eq!(r.identities_dir, PathBuf::from("/flag/ids"));
        assert_eq!(r.identity.as_deref(), Some("flag-id"));
    }

    #[test]
    fn config_file_comes_from_flag_env_then_owo_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = env(tmp.path());
        fs::create_dir_all(tmp.path().join("other")).unwrap();
        fs::write(
            tmp.path().join("other/owo.toml"),
            "identities_dir = \"ids\"\n",
        )
        .unwrap();
        e.owo_dir = Some("dir".into());
        e.owo_config = Some("other/owo.toml".into());
        let r = resolve(&Select::default(), &e).unwrap();
        assert_eq!(r.owo_dir, tmp.path().join("dir"));
        assert_eq!(r.identities_dir, tmp.path().join("other/ids"));
        assert_eq!(r.templates_dir, tmp.path().join("dir/templates"));

        let s = Select {
            config: Some("missing.toml".into()),
            ..Default::default()
        };
        let err = resolve(&s, &e).unwrap_err().to_string();
        assert!(err.contains("missing.toml"), "{err}");

        fs::write(tmp.path().join("other/owo.toml"), "root = \"ids\"\n").unwrap();
        let err = resolve(&Select::default(), &e).unwrap_err().to_string();
        assert!(err.contains("unknown field `root`"), "{err}");
    }

    #[test]
    fn single_identity_is_picked() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("thea")).unwrap();
        fs::write(tmp.path().join("thea/identity.json"), "{}").unwrap();
        let select = Select {
            identities_dir: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        let r = resolve(&select, &env(tmp.path())).unwrap();
        assert_eq!(r.identity.as_deref(), Some("thea"));
    }
}
