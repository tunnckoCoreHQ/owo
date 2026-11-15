//! owo: one world, one identity. The library for living agent identities.
//!
//! Open a root with `Root::open`, load its identity with `Root::identity`, then call the typed
//! function of each operation on the `Root` or the `Identity` with its request type. Each
//! function returns a typed result or an `Error`. A request type lists its fields (`Request`):
//! name, type, help text, default, allowed values and rules, and `operations` lists every
//! operation with its request type, so a caller builds its flags or parameters from them.
//! Each typed function checks its request against the rules declared on its fields, so a request
//! built by hand is as safe as one built from arguments. `mcp` is the MCP server and `acp` the
//! ACP proxy, both on streams the caller gives. The library parses no command line, has no exit
//! codes and reads nothing from the process: the caller passes the environment values in `Env`.
//! The `owo` binary of `owo-cli` is its CLI.

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
pub mod mcp;
mod migrate;
mod operation;
mod output;
mod request;
mod review;
mod search;
mod slug;
mod stamp;
mod state;
mod status;
mod sync;
mod templates;
mod write;

pub use changes::{Change, Changes};
pub use config::{Resolved, Source};
pub use context::{Context, Item as ContextItem};
pub use create::Created;
pub use error::{Error, ErrorKind, Fault, Result};
pub use export::{Report as Soulspec, Spec};
pub use find::{Hit, LinkRef};
pub use identity::{Identity, Kind};
pub use kb::Kb;
pub use lint::Rule;
pub use migrate::Report as MigrateReport;
pub use operation::{operations, Envelope, Operation, Outcome, Ran};
pub use output::{warning_line, Diagnostic, Level};
pub use request::{
    AddRequest, Args, CardRequest, ChangesRequest, ContextRequest, Field, IndexRequest,
    LinksRequest, LintRequest, MigrateRequest, NewRequest, Reader, Request, ResolveRequest,
    ReviewDoneRequest, SearchRequest, ShowRequest, SkillRequest, SlugRequest, SoulHistoryRequest,
    SoulNoteRequest, SoulspecRequest, StampRequest, StateRequest, SyncDoneRequest, SyncOptions,
    SyncPushRequest, SyncStartRequest, TemplateRequest, Type, WriteRequest, MAX_DAYS,
};
pub use review::{Changed, Day, Note as SoulNote, Packet, ReviewRecord, SoulChange};
pub use search::{Hit as SearchHit, Scope};
pub use stamp::Report as StampReport;
pub use state::parse_day;
pub use status::{now_block, Link, ProjectState, Review, Row, State};
pub use sync::{
    parse_turns, Bookmark, Due as SyncDue, Format as SyncFormat, PendingRun, Role,
    Start as SyncRun, Turn,
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
pub fn slug(req: &SlugRequest) -> Result<String> {
    req.check()?;
    let text = &req.text;
    let value = slug::slugify(text);
    if value.is_empty() {
        return Err(Error::new(
            ErrorKind::Failed,
            format!("{text:?} has no letters or digits to make a slug"),
        ));
    }
    Ok(value)
}

/// `path` as valid UTF-8 text, each run of bytes that is not UTF-8 replaced by U+FFFD, so every
/// caller gets the same path for the same argument.
fn utf8(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().into_owned())
}

/// The names of the entry types `add` creates.
pub fn entry_types() -> impl Iterator<Item = &'static str> {
    templates::ENTRY_TEMPLATES.iter().map(|(n, _)| *n)
}

/// Moves a v1 identity folder into a new identity folder (`<root>/<slug>`). Relative paths
/// join onto `cwd`, the caller's folder.
pub fn migrate_v1(req: &MigrateRequest, cwd: &Path) -> Result<MigrateReport> {
    req.check()?;
    Ok(migrate::v1(cwd, &utf8(&req.path), &utf8(&req.out))?)
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

/// An opened root: where it is, which identity and project it selects, and the warnings found
/// while resolving them.
#[derive(Debug)]
pub struct Root {
    pub config: Resolved,
    pub warnings: Vec<String>,
    home: Option<PathBuf>,
    /// `Env.cwd`, which relative request paths join onto.
    cwd: PathBuf,
}

impl Root {
    /// Resolves the root, identity and project from `select`, `env` and the config files. It
    /// reads no identity yet.
    pub fn open(select: &Select, env: &Env) -> Result<Root> {
        let inputs = config::Inputs::new(select, env);
        let config = config::resolve(&inputs).map_err(|e| {
            let mut e = Error::from(e);
            e.kind = ErrorKind::Config;
            e
        })?;
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
            cwd: env.cwd.clone(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.config.root
    }

    /// Loads the selected identity, with the selected project as its working project.
    pub fn identity(&self) -> Result<Identity> {
        let mut id = Identity::load(&self.config.root, &need_identity(&self.config)?)?;
        id.home = self.home.clone();
        id.cwd = self.cwd.clone();
        id.project = self.config.project.clone();
        Ok(id)
    }

    /// One row per identity in the root.
    pub fn list(&self) -> Vec<Row> {
        status::list(&self.config.root)
    }

    /// Creates an identity from the templates.
    pub fn create(&self, req: &NewRequest) -> Result<Created> {
        req.check()?;
        let slug = req.slug.clone().unwrap_or_else(|| slug::slugify(&req.name));
        Ok(create::new_identity(
            &self.config.root,
            &self.config.root,
            req.name.trim(),
            &slug,
        )?)
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
    pub fn template(&self, req: &TemplateRequest) -> Result<(Template, String)> {
        req.check()?;
        let all = templates::all();
        let Some((name, file, builtin)) = all.into_iter().find(|(n, _, _)| *n == req.name) else {
            return Err(Fault::UnknownTemplate(req.name.clone()).into());
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
            cwd: &self.cwd,
            slug,
            project,
            every: self.config.sync_every,
        })
    }

    /// Moves the bookmark of a session after an ingest run. Returns the bookmark.
    pub fn sync_done(&self, req: &SyncDoneRequest) -> Result<Bookmark> {
        req.check()?;
        Ok(sync::done(&self.config.root, &req.session, req.until)?)
    }

    /// The sessions in the root that have a pending run, by session id. With a picked identity
    /// (`Select::identity`), only the runs of that identity.
    pub fn sync_due(&self) -> Result<Vec<SyncDue>> {
        let slug = match self.config.identity_source {
            Some(Source::Flag) => Some(need_identity(&self.config)?),
            _ => None,
        };
        Ok(sync::due(&self.config.root, slug.as_deref())?)
    }
}

/// Session sync for one identity: stores turns and slices them into `sources/` for ingest.
pub struct SessionSync<'a> {
    root: &'a Path,
    /// `Env.cwd`, which a relative transcript path joins onto.
    cwd: &'a Path,
    slug: String,
    project: Option<String>,
    /// `sync_every` from the config.
    every: Option<usize>,
}

impl SessionSync<'_> {
    fn request<'a>(&'a self, session: &'a str, opts: &SyncOptions) -> sync::StartRequest<'a> {
        sync::StartRequest {
            root: self.root,
            cwd: self.cwd,
            slug: &self.slug,
            session,
            now: opts.now,
            every: opts
                .every
                .or(self.every)
                .unwrap_or(request::DEFAULT_SYNC_EVERY)
                .max(1),
            project: self.project.as_deref(),
        }
    }

    /// Reads the host's transcript of the session and, when `every` new user turns arrived or
    /// `now` is set, writes the slice and the ingest prompt.
    pub fn start(&self, req: &SyncStartRequest) -> Result<SyncRun> {
        req.check()?;
        Ok(sync::start(
            &self.request(&req.session, &req.options),
            &utf8(&req.transcript),
            req.format,
        )?)
    }

    /// Stores `turns`, skipping stored ids and turns with no text, then runs `start` on the
    /// stored turns.
    pub fn push(&self, session: &str, turns: &[Turn], opts: &SyncOptions) -> Result<SyncRun> {
        opts.check()?;
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

    /// The lint findings of the rules (every rule when empty). With `changed`, only the
    /// findings in files changed since the last commit or stamp.
    pub fn lint(&self, req: &LintRequest) -> Result<Vec<Diagnostic>> {
        req.check()?;
        let mut found = lint::run(self, &req.rules);
        if req.changed {
            let paths = changes::changed_paths(self);
            found.retain(|d| paths.contains(&d.path));
        }
        Ok(found)
    }

    /// Writes the full text of one authored file. Returns true when the file is new.
    pub fn write(&self, req: &WriteRequest) -> Result<bool> {
        req.check()?;
        let text = req.content.as_deref().unwrap_or_default();
        Ok(add::write(self, &req.path, text)?)
    }

    /// Creates an entry from its template. Returns its path and true when the file is new. A
    /// note goes under the working project, which must exist; other types ignore it.
    pub fn add(&self, req: &AddRequest) -> Result<(String, bool)> {
        req.check()?;
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
    pub fn stamp(&self, req: &StampRequest) -> Result<StampReport> {
        req.check()?;
        Ok(stamp::run(self, req.check)?)
    }

    /// Writes the generated files and returns their paths; with `check`, only returns the paths
    /// of the ones that are out of date.
    pub fn index(&self, req: &IndexRequest) -> Result<Vec<String>> {
        req.check()?;
        if req.check {
            return Ok(index::stale(self));
        }
        Ok(index::write(self)?)
    }

    /// The entry the name resolves to, optionally of one type; else the candidates.
    pub fn resolve(&self, req: &ResolveRequest) -> Result<Result<Hit, Vec<Hit>>> {
        req.check()?;
        let hits = |v: Vec<usize>| v.iter().map(|i| Hit::of(&self.entries[*i])).collect();
        Ok(match find::resolve(self, &req.name, req.kind) {
            find::Resolution::Found(i) => Ok(Hit::of(&self.entries[i])),
            find::Resolution::Ambiguous(v) | find::Resolution::Missing(v) => Err(hits(v)),
        })
    }

    /// The text of an entry or a `kb:` link, or one section of it, with its path.
    pub fn show(&self, req: &ShowRequest) -> Result<(String, String)> {
        req.check()?;
        Ok(find::show(self, &req.name, req.section.as_deref())?)
    }

    /// The links into and out of the entry. Both when neither is asked for. `orphans` is for
    /// the operation, which calls `Identity::orphans` instead.
    pub fn links(&self, req: &LinksRequest) -> Result<Links> {
        req.check()?;
        let i = find::one(self, &req.name)?;
        let both = !req.inbound && !req.outbound;
        Ok(Links {
            path: self.entries[i].rel.clone(),
            inbound: if req.inbound || both {
                find::backlinks(self, i)
            } else {
                vec![]
            },
            outbound: if req.outbound || both {
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
        req.check()?;
        let project = self.working_project()?;
        Ok(context::build(self, req, project)?)
    }

    /// The text of a load set: each file after an `<!-- owo: ... -->` line, the KB pointer, and
    /// the Now block last.
    pub fn context_text(&self, ctx: &Context) -> String {
        context::render_text(self, ctx)
    }

    /// The derived current state, with events of the last `req.days`, and the state of the
    /// working project, which must exist.
    pub fn state(&self, req: &StateRequest) -> Result<State> {
        req.check()?;
        let mut s = status::compute(self, req.days, 5);
        if let Some(p) = self.working_project()? {
            s.project = Some(status::project(self, p));
        }
        Ok(s)
    }

    /// The BM25 hits for the query, best first, at most `req.limit`.
    pub fn search(&self, req: &SearchRequest) -> Result<Vec<SearchHit>> {
        req.check()?;
        Ok(search::search(self, req))
    }

    /// The review packet.
    pub fn review(&self) -> Packet {
        review::packet(self)
    }

    /// Records a finished review through the story day `through`, else the newest one. Returns
    /// the review record.
    pub fn review_done(&self, req: &ReviewDoneRequest) -> Result<ReviewRecord> {
        req.check()?;
        Ok(review::done(self, req.through)?)
    }

    /// Appends a soul change to the soul history, with its evidence entries.
    pub fn soul_note(&self, req: &SoulNoteRequest) -> Result<SoulNote> {
        req.check()?;
        Ok(review::soul_note(
            self,
            &req.file,
            &req.change,
            &req.evidence,
        )?)
    }

    /// The soul history, of one soul file or of all.
    pub fn soul_history(&self, req: &SoulHistoryRequest) -> Result<Vec<SoulChange>> {
        req.check()?;
        Ok(review::soul_history(self, req.file.as_deref()))
    }

    /// What changed, grouped by day, since `req.since`, or in the last `req.days` (from the
    /// local day that many days ago).
    pub fn changes(&self, req: &ChangesRequest) -> Result<Changes> {
        req.check()?;
        let since = req
            .since
            .clone()
            .or_else(|| req.days.map(state::local_day_ago));
        Ok(changes::changes(self, since.as_deref())?)
    }

    /// Builds a character card, and writes it when `req.out` is set.
    pub fn export_card(&self, req: &CardRequest) -> Result<Card> {
        req.check()?;
        let project = self.check_project(req.project.as_deref())?;
        let scenario = req.scenario.as_deref();
        let card = export::card(self, req.spec, project, scenario)?;
        let warnings = export::warnings(&card);
        if let Some(out) = req.out.as_deref().map(utf8) {
            let png = req.png.as_deref().map(|p| self.cwd.join(utf8(p)));
            export::write_card(
                self,
                &card,
                project,
                scenario,
                &self.cwd.join(&out),
                &out,
                png.as_deref(),
            )?;
        }
        Ok(Card { card, warnings })
    }

    /// Writes a SKILL.md wrapper into `req.out` and returns its path, as `req.out` gives it.
    pub fn export_skill(&self, req: &SkillRequest) -> Result<String> {
        req.check()?;
        let out = utf8(&req.out);
        export::skill(self, &self.cwd.join(&out))?;
        Ok(out.join("SKILL.md").display().to_string())
    }

    /// Writes a SoulSpec package into `req.out`.
    pub fn export_soulspec(&self, req: &SoulspecRequest) -> Result<Soulspec> {
        req.check()?;
        let out = utf8(&req.out);
        Ok(export::soulspec(self, &self.cwd.join(&out), &out)?)
    }
}
