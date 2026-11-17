use crate::{Env, Error, ErrorKind, Fault, Result, Select};
use anyhow::Context;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const LOCAL_CONFIG: &str = ".owo.toml";
pub const GLOBAL_CONFIG: &str = "owo.toml";

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
pub enum Origin {
    Flag,
    Env,
    LocalConfig(PathBuf),
    GlobalConfig(PathBuf),
    Default,
    OnlyIdentity,
}

#[derive(Debug, serde::Serialize)]
pub struct Resolved {
    pub root: PathBuf,
    pub root_source: Origin,
    pub identity: Option<String>,
    pub identity_source: Option<Origin>,
    pub project: Option<String>,
    pub sync_every: Option<usize>,
    pub auto_ingest: bool,
    pub ingest_command: Option<Vec<String>>,
}

type Loaded = Option<(Origin, FileConfig)>;

pub fn expand_home(value: &str, home: Option<&Path>) -> PathBuf {
    match (value.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(value),
    }
}

fn load(path: Option<PathBuf>, origin: fn(PathBuf) -> Origin) -> anyhow::Result<Loaded> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let config = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some((origin(path), config)))
}

pub fn find_local_config(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(LOCAL_CONFIG))
        .find(|p| p.is_file())
}

fn lookup(
    flag: Option<String>,
    env: Option<&String>,
    files: &[Loaded],
    field: fn(&FileConfig) -> Option<&String>,
) -> Option<(String, Origin)> {
    flag.map(|v| (v, Origin::Flag))
        .or_else(|| env.map(|v| (v.clone(), Origin::Env)))
        .or_else(|| {
            files
                .iter()
                .flatten()
                .find_map(|(origin, c)| Some((field(c)?.clone(), origin.clone())))
        })
}

pub fn resolve(select: &Select, env: &Env) -> Result<Resolved> {
    let home = env
        .home
        .as_deref()
        .filter(|h| h.to_str().is_some_and(|s| !s.is_empty()));
    let local = load(find_local_config(&env.cwd), Origin::LocalConfig)?;

    let root_flag = select
        .root
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    let (root, root_source) = match lookup(
        root_flag,
        env.owo_root.as_ref(),
        std::slice::from_ref(&local),
        |c| c.root.as_ref(),
    ) {
        Some((value, origin)) => {
            let base = match &origin {
                Origin::LocalConfig(p) => p.parent().unwrap_or(Path::new(".")),
                _ => env.cwd.as_path(),
            };
            (base.join(expand_home(&value, home)), origin)
        }
        None => (
            env.xdg_data_home
                .clone()
                .or_else(|| home.map(|h| h.join(".local/share")))
                .ok_or(Fault::NoRoot)?
                .join("owo"),
            Origin::Default,
        ),
    };
    let global = load(
        Some(root.join(GLOBAL_CONFIG)).filter(|p| p.is_file()),
        Origin::GlobalConfig,
    )?;
    let configs = [local, global];

    let (identity, identity_source) = lookup(
        select.identity.clone(),
        env.owo_identity.as_ref(),
        &configs,
        |c| c.identity.as_ref(),
    )
    .or_else(|| only_identity(&root).map(|slug| (slug, Origin::OnlyIdentity)))
    .unzip();
    let [local, _global] = &configs;
    let project = lookup(
        select.project.clone(),
        env.owo_project.as_ref(),
        std::slice::from_ref(local),
        |c| c.project.as_ref(),
    )
    .map(|(value, _)| value);

    Ok(Resolved {
        root,
        root_source,
        identity,
        identity_source,
        project,
        sync_every: first(&configs, |c| c.sync_every),
        auto_ingest: first(&configs, |c| c.auto_ingest).unwrap_or(false),
        ingest_command: first(&configs, |c| c.ingest_command.clone()),
    })
}

fn first<T>(configs: &[Loaded], field: impl Fn(&FileConfig) -> Option<T>) -> Option<T> {
    configs.iter().flatten().find_map(|(_, c)| field(c))
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
    let mut slugs = list_identities(root);
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
        .ok_or_else(|| Fault::NoIdentity(r.root.clone()))?;
    check_slug("identity", &slug)?;
    if !r.root.join(&slug).is_dir() {
        return Err(Fault::NoIdentityFolder(r.root.join(&slug)).into());
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
    fn default_root_uses_xdg_data_home_then_home() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = env(tmp.path());
        assert_eq!(
            resolve(&Select::default(), &e).unwrap().root,
            tmp.path().join("home/.local/share/owo")
        );
        e.xdg_data_home = Some(tmp.path().join("data"));
        assert_eq!(
            resolve(&Select::default(), &e).unwrap().root,
            tmp.path().join("data/owo")
        );
    }

    #[test]
    fn precedence_flag_env_local_global() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("home/.local/share/owo");
        fs::create_dir_all(&data).unwrap();
        fs::write(
            data.join(GLOBAL_CONFIG),
            "root = \"~/ids\"\nidentity = \"global-pick\"\nsync_every = 3\n",
        )
        .unwrap();
        let mut s = Select::default();
        let mut e = env(tmp.path());
        let r = resolve(&s, &e).unwrap();
        assert_eq!(r.root, data, "the global config sets no root");
        assert_eq!(r.identity.as_deref(), Some("global-pick"));
        assert_eq!(r.sync_every, Some(3));

        let proj = tmp.path().join("proj/sub");
        fs::create_dir_all(&proj).unwrap();
        fs::create_dir_all(tmp.path().join("proj/ids")).unwrap();
        fs::write(
            tmp.path().join("proj/ids").join(GLOBAL_CONFIG),
            "identity = \"global-two\"\nsync_every = 4\n",
        )
        .unwrap();
        fs::write(
            tmp.path().join("proj/.owo.toml"),
            "root = \"ids\"\nidentity = \"thea\"\nproject = \"api\"\n",
        )
        .unwrap();
        e.cwd = proj;
        let r = resolve(&s, &e).unwrap();
        assert_eq!(r.root, tmp.path().join("proj/ids"));
        assert_eq!(r.identity.as_deref(), Some("thea"));
        assert_eq!(r.project.as_deref(), Some("api"));
        assert_eq!(r.sync_every, Some(4), "the global config of that root");

        e.owo_root = Some("/env/root".into());
        e.owo_identity = Some("env-id".into());
        let r = resolve(&s, &e).unwrap();
        assert_eq!(r.root, PathBuf::from("/env/root"));
        assert_eq!(r.identity.as_deref(), Some("env-id"));

        s.root = Some(PathBuf::from("/flag/root"));
        s.identity = Some("flag-id".into());
        let r = resolve(&s, &e).unwrap();
        assert_eq!(r.root, PathBuf::from("/flag/root"));
        assert_eq!(r.identity.as_deref(), Some("flag-id"));
    }

    #[test]
    fn single_identity_is_picked() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("thea")).unwrap();
        fs::write(tmp.path().join("thea/identity.json"), "{}").unwrap();
        let select = Select {
            root: Some(tmp.path().to_path_buf()),
            ..Default::default()
        };
        let r = resolve(&select, &env(tmp.path())).unwrap();
        assert_eq!(r.identity.as_deref(), Some("thea"));
    }
}
