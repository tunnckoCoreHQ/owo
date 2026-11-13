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
mod error;
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
pub use error::{Error, ErrorKind, Fault, Result};
pub use export::{Report as Soulspec, Spec};
pub use find::{Hit, LinkRef};
pub use identity::{Identity, Kind};
pub use kb::Kb;
pub use lint::Rule;
pub use migrate::Report as MigrateReport;
pub use output::{Diagnostic, Level};
pub use review::{Changed, Day, Note as SoulNote, Packet, ReviewRecord, SoulChange};
pub use search::{Hit as SearchHit, Request as SearchRequest, Scope};
pub use stamp::Report as StampReport;
pub use state::parse_day;
pub use status::{now_block, Link, ProjectState, Review, Row, State};
pub use sync::{
    parse_turns, Bookmark, Format as SyncFormat, PendingRun, Role, Start as SyncRun, Turn,
};

use config::{need_identity, need_project};
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

/// The slug of a text; an error when the text has no letters or digits.
pub fn slug(text: &str) -> Result<String> {
    let value = slug::slugify(text);
    if value.is_empty() {
        return Err(Error::new(
            ErrorKind::Failed,
            format!("{text:?} has no letters or digits to make a slug"),
        ));
    }
    Ok(value)
}

/// The names of the entry types `add` creates.
pub fn entry_types() -> impl Iterator<Item = &'static str> {
    templates::ENTRY_TEMPLATES.iter().map(|(n, _)| *n)
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
    pub spec: Spec,
    /// The project whose notes join the card; it must exist.
    pub project: Option<&'a str>,
    /// The event or location whose summary becomes the scenario.
    pub scenario: Option<&'a str>,
    /// Writes the card to this file when set.
    pub out: Option<&'a Path>,
    /// Embeds the card in this PNG avatar; needs `out`.
    pub png: Option<&'a Path>,
}

impl Default for CardRequest<'_> {
    /// A V3 card of the whole identity, not written.
    fn default() -> Self {
        Self {
            spec: Spec::V3,
            project: None,
            scenario: None,
            out: None,
            png: None,
        }
    }
}

/// The largest number of days `state` and `changes` take.
pub const MAX_DAYS: i64 = 36500;

/// A number of days from 0 to `MAX_DAYS`.
fn check_days(days: i64) -> Result<i64> {
    if !(0..=MAX_DAYS).contains(&days) {
        return Err(Fault::DaysOutOfRange(days).into());
    }
    Ok(days)
}

/// What `state` derives.
#[derive(Debug, Clone, Copy)]
pub struct StateRequest {
    /// The window for recent events, from 0 to `MAX_DAYS`.
    pub days: i64,
}

impl Default for StateRequest {
    /// Events of the last 14 days.
    fn default() -> Self {
        Self { days: 14 }
    }
}

/// Which changes `changes` lists: all by default.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChangesRequest<'a> {
    /// A day, or what `git log --since` takes.
    pub since: Option<&'a str>,
    /// Only the last N days, from 0 to `MAX_DAYS`; not with `since`.
    pub days: Option<i64>,
}

/// When a sync step writes a slice for ingest.
#[derive(Debug, Clone, Copy)]
pub struct SyncOptions {
    /// Take any new turns now (session end).
    pub now: bool,
    /// New user turns per run.
    pub every: usize,
}

impl Default for SyncOptions {
    /// A run every six new user turns.
    fn default() -> Self {
        Self {
            now: false,
            every: 6,
        }
    }
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
        let config = config::resolve(&inputs)
            .map_err(|e| Error::new(ErrorKind::Config, format!("{e:#}")))?;
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

    /// Loads the selected identity, with the selected project as its working project.
    pub fn identity(&self) -> Result<Identity> {
        let mut id = Identity::load(&self.config.root, &need_identity(&self.config)?)?;
        id.home = self.home.clone();
        id.project = self.config.project.clone();
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
            return Err(Fault::UnknownTemplate(name.to_string()).into());
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
    pub fn sync_done(&self, session: &str, until: usize) -> Result<Bookmark> {
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
    fn request<'a>(&'a self, session: &'a str, opts: &SyncOptions) -> sync::StartRequest<'a> {
        sync::StartRequest {
            root: self.root,
            slug: &self.slug,
            session,
            now: opts.now,
            every: opts.every.max(1),
            project: self.project.as_deref(),
        }
    }

    /// Reads the host's transcript of `session` in `format` and, when `opts.every` new user
    /// turns arrived or `opts.now` is set, writes the slice and the ingest prompt.
    pub fn start(
        &self,
        session: &str,
        transcript: &Path,
        format: SyncFormat,
        opts: &SyncOptions,
    ) -> Result<SyncRun> {
        Ok(sync::start(
            &self.request(session, opts),
            transcript,
            format,
        )?)
    }

    /// Stores `turns`, skipping stored ids and turns with no text, then runs `start` on the
    /// stored turns.
    pub fn push(&self, session: &str, turns: &[Turn], opts: &SyncOptions) -> Result<SyncRun> {
        Ok(sync::push(&self.request(session, opts), turns)?)
    }
}

impl Identity {
    /// `project` when it is a project of this identity; an error when it is not.
    fn check_project<'a>(&self, project: Option<&'a str>) -> Result<Option<&'a str>> {
        Ok(need_project(&self.dir, project)?)
    }

    /// The working project, checked to be a project of this identity.
    pub fn working_project(&self) -> Result<Option<&str>> {
        self.check_project(self.project.as_deref())
    }

    /// The lint findings of the `rules` (every rule when empty). With `changed`, only the
    /// findings in files changed since the last commit or stamp.
    pub fn lint(&self, rules: &[Rule], changed: bool) -> Vec<Diagnostic> {
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
    /// note goes under the working project, which must exist; other types ignore it.
    pub fn add(&self, req: &AddRequest) -> Result<(String, bool)> {
        if req.kind.type_name().is_none() {
            return Err(Error::new(
                ErrorKind::Usage,
                format!(
                    "{} is not an entry type; `entry_types` lists them",
                    req.kind.name()
                ),
            ));
        }
        if req.title.trim().is_empty() && req.kind != Kind::Log {
            return Err(Fault::NeedsTitle(req.kind).into());
        }
        let project = if req.kind == Kind::Note {
            self.working_project()?
        } else {
            None
        };
        Ok(add::add(self, req, project)?)
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
    pub fn resolve(&self, name: &str, kind: Option<Kind>) -> Result<Hit, Vec<Hit>> {
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

    /// The tiered load set, with the working project, which must exist.
    pub fn context(&self, req: &ContextRequest) -> Result<Context> {
        let project = self.working_project()?;
        Ok(context::build(self, req, project)?)
    }

    /// The text of a load set: each file after an `<!-- owo: ... -->` line, the Now block and the
    /// KB pointer.
    pub fn context_text(&self, ctx: &Context) -> String {
        context::render_text(self, ctx)
    }

    /// The derived current state, with events of the last `req.days`, and the state of the
    /// working project, which must exist.
    pub fn state(&self, req: &StateRequest) -> Result<State> {
        let mut s = status::compute(self, check_days(req.days)?, 5);
        if let Some(p) = self.working_project()? {
            s.project = Some(status::project(self, p));
        }
        Ok(s)
    }

    /// The BM25 hits for `query`, best first, at most `req.limit`.
    pub fn search(&self, query: &str, req: &SearchRequest) -> Vec<SearchHit> {
        search::search(self, query, req)
    }

    /// The review packet.
    pub fn review(&self) -> Packet {
        review::packet(self)
    }

    /// Records a finished review through the story day `through`, else the newest one. Returns
    /// the review record.
    pub fn review_done(&self, through: Option<chrono::NaiveDate>) -> Result<ReviewRecord> {
        Ok(review::done(self, through)?)
    }

    /// Appends a soul change to the soul history, with its evidence entries.
    pub fn soul_note(&self, file: &str, change: &str, evidence: &[String]) -> Result<SoulNote> {
        Ok(review::soul_note(self, file, change, evidence)?)
    }

    /// The soul history, of one soul file or of all.
    pub fn soul_history(&self, file: Option<&str>) -> Vec<SoulChange> {
        review::soul_history(self, file)
    }

    /// What changed, grouped by day, since `req.since`, or in the last `req.days` (from the
    /// local day that many days ago).
    pub fn changes(&self, req: &ChangesRequest) -> Result<Changes> {
        let since = match (req.since, req.days) {
            (Some(_), Some(_)) => return Err(Fault::SinceWithDays.into()),
            (Some(s), None) => Some(s.to_string()),
            (None, Some(days)) => Some(state::local_day_ago(check_days(days)?)),
            (None, None) => None,
        };
        Ok(changes::changes(self, since.as_deref())?)
    }

    /// Builds a character card, and writes it when `req.out` is set.
    pub fn export_card(&self, req: &CardRequest) -> Result<Card> {
        if req.png.is_some() && req.out.is_none() {
            return Err(Fault::PngNeedsOut.into());
        }
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
