//! owo: one world, one identity. The library for living agent identities.
//!
//! Open a root with `Root::open`, load its identity with `Root::identity`, then call the typed
//! function of each command on the `Root` or the `Identity`. Each function takes typed
//! arguments and returns a typed result or an `Error`. The library parses no command line,
//! renders no output, has no exit codes and reads nothing from the process: the caller passes
//! the environment values in `Env`. `acp` is the ACP proxy, a consumer of this API, on streams
//! the caller gives it. The `owo` binary, with its CLI and MCP server, is another consumer.

pub mod acp;
mod add;
mod changes;
mod config;
mod context;
mod create;
mod export;
mod find;
mod git;
mod identity;
mod index;
mod kb;
mod lint;
mod markdown;
mod migrate;
mod output;
mod review;
mod search;
mod slug;
mod stamp;
mod state;
mod status;
mod sync;
mod templates;

pub use add::Request as AddRequest;
pub use changes::{Change, Changes};
pub use config::{Resolved, Source};
pub use context::{Context, Item as ContextItem, Request as ContextRequest};
pub use create::Created;
pub use export::Report as Soulspec;
pub use find::{unresolved, Hit, LinkRef};
pub use identity::{Identity, Kind};
pub use kb::Kb;
pub use migrate::Report as MigrateReport;
pub use output::{Diagnostic, Level};
pub use review::{Changed, Day, Note as SoulNote, Packet};
pub use search::{Filter as SearchFilter, Hit as SearchHit};
pub use stamp::Report as StampReport;
pub use state::{day, parse_day};
pub use status::{join as join_links, now_block, Link, ProjectState, Review, Row, State};
pub use sync::{Start as SyncRun, FORMATS as SYNC_FORMATS};

use config::{need_identity, need_project, ConfigError};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// What the library would read from a process: environment values and the working folder.
/// The caller reads them once and passes them in.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `OWO_ROOT`, `OWO_IDENTITY` and `OWO_PROJECT`, when set and not empty.
    pub owo_root: Option<String>,
    pub owo_identity: Option<String>,
    pub owo_project: Option<String>,
    pub cwd: PathBuf,
    /// `HOME` as set.
    pub home: Option<PathBuf>,
    /// `XDG_CONFIG_HOME` and `XDG_DATA_HOME`, when set and not empty.
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
}

/// The root, identity and project the caller picks, before `Env` and the config files.
#[derive(Debug, Clone, Default)]
pub struct Select {
    pub root: Option<PathBuf>,
    pub identity: Option<String>,
    pub project: Option<String>,
}

/// Why a call failed. The message is the whole error text.
#[derive(Debug)]
pub enum Error {
    /// A bad argument, such as `add` with no title.
    Usage(String),
    /// A root, identity or project that does not resolve or does not exist.
    Config(String),
    /// A file that cannot be read or written.
    Io(String),
    /// Anything else the call could not do.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Usage(m) | Error::Config(m) | Error::Io(m) | Error::Failed(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        let message = format!("{e:#}");
        if e.downcast_ref::<ConfigError>().is_some() {
            Error::Config(message)
        } else if e.downcast_ref::<std::io::Error>().is_some() {
            Error::Io(message)
        } else {
            Error::Failed(message)
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The slug of a text; an error when the text has no letters or digits.
pub fn slug(text: &str) -> Result<String> {
    let value = slug::slugify(text);
    if value.is_empty() {
        return Err(Error::Failed(format!(
            "{text:?} has no letters or digits to make a slug"
        )));
    }
    Ok(value)
}

/// The names of the entry types `add` creates.
pub fn entry_types() -> impl Iterator<Item = &'static str> {
    templates::ENTRY_TEMPLATES.iter().map(|(n, _)| *n)
}

/// The names of the lint rules.
pub fn lint_rules() -> impl Iterator<Item = &'static str> {
    lint::rules()
}

/// Moves a v1 identity folder at `path` into a new identity folder `out` (`<root>/<slug>`).
pub fn migrate_v1(path: &Path, out: &Path) -> Result<MigrateReport> {
    Ok(migrate::v1(path, out)?)
}

/// One template: its name and its override file in `<root>/.templates/`, when there is one.
#[derive(Debug, serde::Serialize)]
pub struct Template {
    pub name: &'static str,
    #[serde(rename = "override")]
    pub override_path: Option<PathBuf>,
}

/// The backlinks and outlinks of one entry.
#[derive(Debug)]
pub struct Links {
    pub path: String,
    pub inbound: Vec<LinkRef>,
    pub outbound: Vec<LinkRef>,
}

/// A built character card and its warnings.
#[derive(Debug)]
pub struct Card {
    pub card: Value,
    pub warnings: Vec<String>,
}

/// What `export_card` builds, and where it writes the card.
pub struct CardRequest<'a> {
    /// `v2` or `v3`.
    pub spec: &'a str,
    /// The project whose notes join the card; it must exist.
    pub project: Option<&'a str>,
    /// The event or location whose summary becomes the scenario.
    pub scenario: Option<&'a str>,
    /// Writes the card to this file when set.
    pub out: Option<&'a Path>,
    /// Embeds the card in this PNG avatar; needs `out`.
    pub png: Option<&'a Path>,
}

/// An opened root: where it is, which identity and project it selects, and the warnings found
/// while resolving them.
#[derive(Debug)]
pub struct Root {
    pub config: Resolved,
    pub warnings: Vec<String>,
    home: Option<PathBuf>,
}

impl Root {
    /// Resolves the root, identity and project from `select`, `env` and the config files. It
    /// reads no identity yet.
    pub fn open(select: &Select, env: &Env) -> Result<Root> {
        let inputs = config::Inputs::new(select, env);
        let config = config::resolve(&inputs).map_err(|e| Error::Config(format!("{e:#}")))?;
        let mut warnings = vec![];
        if let Source::ProjectFile(file) = &config.root_source {
            if file.parent() != Some(inputs.cwd.as_path()) {
                warnings.push(format!(
                    "the root comes from {} in a parent folder",
                    file.display()
                ));
            }
        }
        Ok(Root {
            config,
            warnings,
            home: env.home.clone(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.config.root
    }

    /// Loads the selected identity.
    pub fn identity(&self) -> Result<Identity> {
        let mut id = Identity::load(&self.config.root, &need_identity(&self.config)?)?;
        id.home = self.home.clone();
        Ok(id)
    }

    /// One row per identity in the root.
    pub fn list(&self) -> Vec<Row> {
        status::list(&self.config.root)
    }

    /// Creates an identity named `name` from the templates, with `slug` or the slug of the name.
    pub fn create(&self, name: &str, slug: Option<&str>) -> Result<Created> {
        let slug = slug.map_or_else(|| slug::slugify(name), str::to_string);
        Ok(create::new_identity(&self.config.root, name.trim(), &slug)?)
    }

    /// Every template.
    pub fn templates(&self) -> Vec<Template> {
        templates::all()
            .into_iter()
            .map(|(name, file, _)| Template {
                name,
                override_path: templates::override_path(&self.config.root, &file),
            })
            .collect()
    }

    /// One template and its text, as `create` and `add` use it.
    pub fn template(&self, name: &str) -> Result<(Template, String)> {
        let all = templates::all();
        let Some((name, file, builtin)) = all.into_iter().find(|(n, _, _)| *n == name) else {
            return Err(Error::Failed(format!(
                "unknown template {name:?}; run `owo templates` for the list"
            )));
        };
        let root = &self.config.root;
        let template = Template {
            name,
            override_path: templates::override_path(root, &file),
        };
        Ok((template, templates::get(root, &file, builtin)))
    }

    /// The session sync of the selected identity and project, checked to exist.
    pub fn sync(&self) -> Result<SessionSync<'_>> {
        let slug = need_identity(&self.config)?;
        let project = need_project(
            &self.config.root.join(&slug),
            self.config.project.as_deref(),
        )?
        .map(str::to_string);
        Ok(SessionSync {
            root: &self.config.root,
            slug,
            project,
        })
    }

    /// Moves the bookmark of `session` to message `until` after an ingest run. Returns the
    /// bookmark.
    pub fn sync_done(&self, session: &str, until: usize) -> Result<Value> {
        Ok(sync::done(&self.config.root, session, until)?)
    }
}

/// Session sync for one identity: stores turns and slices them into `sources/` for ingest.
pub struct SessionSync<'a> {
    root: &'a Path,
    slug: String,
    project: Option<String>,
}

impl SessionSync<'_> {
    fn request<'a>(&'a self, session: &'a str, now: bool, every: usize) -> sync::StartRequest<'a> {
        sync::StartRequest {
            root: self.root,
            slug: &self.slug,
            session,
            now,
            every: every.max(1),
            project: self.project.as_deref(),
        }
    }

    /// Reads the host's transcript of `session` in `format` (one of `SYNC_FORMATS`) and, when
    /// `every` new user turns arrived or `now` is set, writes the slice and the ingest prompt.
    pub fn start(
        &self,
        session: &str,
        transcript: &Path,
        format: &str,
        now: bool,
        every: usize,
    ) -> Result<SyncRun> {
        Ok(sync::start(
            &self.request(session, now, every),
            transcript,
            format,
        )?)
    }

    /// Stores `turns`, a JSON array of `{id, role, text}`, skipping stored ids, then runs
    /// `start` on the stored turns.
    pub fn push(&self, session: &str, turns: &str, now: bool, every: usize) -> Result<SyncRun> {
        Ok(sync::push(&self.request(session, now, every), turns)?)
    }
}

impl Identity {
    /// `project` when it is a project of this identity; an error when it is not.
    pub fn check_project<'a>(&self, project: Option<&'a str>) -> Result<Option<&'a str>> {
        Ok(need_project(&self.dir, project)?)
    }

    /// The lint findings of the `rules` (every rule when empty). With `changed`, only the
    /// findings in files changed since the last commit or stamp.
    pub fn lint(&self, rules: &[String], changed: bool) -> Vec<Diagnostic> {
        let mut found = lint::run(self, rules);
        if changed {
            let paths = changes::changed_paths(self);
            found.retain(|d| paths.contains(&d.path));
        }
        found
    }

    /// Writes the full text of one authored file. Returns true when the file is new.
    pub fn write(&self, path: &str, text: &str) -> Result<bool> {
        Ok(add::write(self, path, text)?)
    }

    /// Creates an entry from its template. Returns its path and true when the file is new. A
    /// note goes under `req.project`, which must exist; other types ignore it.
    pub fn add(&self, req: &AddRequest) -> Result<(String, bool)> {
        if req.kind.type_name().is_none() {
            return Err(Error::Usage(format!(
                "{} is not an entry type; `entry_types` lists them",
                req.kind.name()
            )));
        }
        if req.title.trim().is_empty() && req.kind != Kind::Log {
            return Err(Error::Usage(format!(
                "`owo add {}` needs a title",
                req.kind.name()
            )));
        }
        let project = if req.kind == Kind::Note {
            self.check_project(req.project)?
        } else {
            None
        };
        Ok(add::add(self, &AddRequest { project, ..*req })?)
    }

    /// Refreshes the state files, or with `check` only reports the drift.
    pub fn stamp(&self, check: bool) -> Result<StampReport> {
        Ok(stamp::run(self, check)?)
    }

    /// Writes the generated files and returns their paths.
    pub fn index(&self) -> Result<Vec<String>> {
        Ok(index::write(self)?)
    }

    /// The generated files that are out of date.
    pub fn stale_index(&self) -> Vec<String> {
        index::stale(self)
    }

    /// The entry `name` resolves to, optionally of one type; else the candidates.
    pub fn resolve(&self, name: &str, kind: Option<&str>) -> Result<Hit, Vec<Hit>> {
        let hits = |v: Vec<usize>| v.iter().map(|i| Hit::of(&self.entries[*i])).collect();
        match find::resolve(self, name, kind) {
            find::Resolution::Found(i) => Ok(Hit::of(&self.entries[i])),
            find::Resolution::Ambiguous(v) | find::Resolution::Missing(v) => Err(hits(v)),
        }
    }

    /// The text of an entry or a `kb:` link, or one section of it, with its path.
    pub fn show(&self, name: &str, section: Option<&str>) -> Result<(String, String)> {
        Ok(find::show(self, name, section)?)
    }

    /// The links into and out of the entry `name`. Both when neither is asked for.
    pub fn links(&self, name: &str, inbound: bool, outbound: bool) -> Result<Links> {
        let i = find::one(self, name)?;
        let both = !inbound && !outbound;
        Ok(Links {
            path: self.entries[i].rel.clone(),
            inbound: if inbound || both {
                find::backlinks(self, i)
            } else {
                vec![]
            },
            outbound: if outbound || both {
                find::outlinks(self, i)
            } else {
                vec![]
            },
        })
    }

    /// The entries nothing links to.
    pub fn orphans(&self) -> Vec<Hit> {
        find::orphans(self)
            .into_iter()
            .map(|i| Hit::of(&self.entries[i]))
            .collect()
    }

    /// The tiered load set. `req.project` must exist.
    pub fn context(&self, req: &ContextRequest) -> Result<Context> {
        let project = self.check_project(req.project)?;
        Ok(context::build(self, &ContextRequest { project, ..*req })?)
    }

    /// The text of a load set: each file after an `<!-- owo: ... -->` line, the Now block and the
    /// KB pointer.
    pub fn context_text(&self, ctx: &Context) -> String {
        context::render_text(self, ctx)
    }

    /// The derived current state, with events of the last `days`, and the state of `project`,
    /// which must exist.
    pub fn state(&self, days: i64, project: Option<&str>) -> Result<State> {
        let mut s = status::compute(self, days, 5);
        if let Some(p) = self.check_project(project)? {
            s.project = Some(status::project(self, p));
        }
        Ok(s)
    }

    /// The BM25 hits for `query`, best first, at most `limit`.
    pub fn search(&self, query: &str, filter: &SearchFilter, limit: usize) -> Vec<SearchHit> {
        search::search(self, query, filter, limit)
    }

    /// The review packet.
    pub fn review(&self) -> Packet {
        review::packet(self)
    }

    /// Records a finished review through the story day `through`, else the newest one. Returns
    /// the review record.
    pub fn review_done(&self, through: Option<chrono::NaiveDate>) -> Result<Value> {
        Ok(review::done(self, through)?)
    }

    /// Appends a soul change to the soul history, with its evidence entries.
    pub fn soul_note(&self, file: &str, change: &str, evidence: &[String]) -> Result<SoulNote> {
        Ok(review::soul_note(self, file, change, evidence)?)
    }

    /// The soul history, of one soul file or of all.
    pub fn soul_history(&self, file: Option<&str>) -> Vec<Value> {
        review::soul_history(self, file)
    }

    /// What changed, grouped by day, since `since` (a day, or what `git log --since` takes).
    pub fn changes(&self, since: Option<&str>) -> Result<Changes> {
        Ok(changes::changes(self, since)?)
    }

    /// Builds a character card, and writes it when `req.out` is set.
    pub fn export_card(&self, req: &CardRequest) -> Result<Card> {
        let project = self.check_project(req.project)?;
        let card = export::card(self, req.spec, project, req.scenario)?;
        let warnings = export::warnings(&card);
        if let Some(out) = req.out {
            export::write_card(self, &card, project, req.scenario, out, req.png)?;
        }
        Ok(Card { card, warnings })
    }

    /// Writes a SKILL.md wrapper into `dir` and returns its path.
    pub fn export_skill(&self, dir: &Path) -> Result<String> {
        Ok(export::skill(self, dir)?)
    }

    /// Writes a SoulSpec package into `dir`.
    pub fn export_soulspec(&self, dir: &Path) -> Result<Soulspec> {
        Ok(export::soulspec(self, dir)?)
    }
}
