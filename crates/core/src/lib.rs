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
    SoulNoteRequest, SoulRequest, SoulspecRequest, StampRequest, StateRequest, SyncDoneRequest,
    SyncOptions, SyncPushRequest, SyncStartRequest, TemplateRequest, Type, WriteRequest, MAX_DAYS,
};
pub use review::{Changed, Day, Note as SoulNote, Packet, ReviewRecord, SoulChange};
pub use search::{Hit as SearchHit, Scope};
pub use stamp::Report as StampReport;
pub use state::{day, parse_day};
pub use status::{join_links, now_block, Link, ProjectState, Review, Row, State};
pub use sync::{
    parse_turns, Bookmark, Due as SyncDue, Format as SyncFormat, PendingRun, Role,
    Start as SyncRun, Turn,
};

use config::{need_identity, need_project};
use serde_json::Value;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub owo_root: Option<String>,
    pub owo_identity: Option<String>,
    pub owo_project: Option<String>,
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct Select {
    pub root: Option<PathBuf>,
    pub identity: Option<String>,
    pub project: Option<String>,
}

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

fn utf8(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().into_owned())
}

pub fn entry_types() -> impl Iterator<Item = &'static str> {
    templates::ENTRY_TEMPLATES.iter().map(|(n, _)| *n)
}

pub fn migrate_v1(req: &MigrateRequest, cwd: &Path) -> Result<MigrateReport> {
    req.check()?;
    Ok(migrate::v1(cwd, &utf8(&req.path), &utf8(&req.out))?)
}

#[derive(Debug, serde::Serialize)]
pub struct Template {
    pub name: &'static str,
    #[serde(rename = "override")]
    pub override_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Links {
    pub path: String,
    pub inbound: Vec<LinkRef>,
    pub outbound: Vec<LinkRef>,
}

#[derive(Debug)]
pub struct Card {
    pub card: Value,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct Root {
    pub config: Resolved,
    pub warnings: Vec<String>,
    home: Option<PathBuf>,
    cwd: PathBuf,
}

impl Root {
    pub fn open(select: &Select, env: &Env) -> Result<Root> {
        let config = config::resolve(select, env).map_err(|mut e| {
            e.kind = ErrorKind::Config;
            e
        })?;
        let mut warnings = vec![];
        if let Source::ProjectFile(file) = &config.root_source {
            if file.parent() != Some(env.cwd.as_path()) {
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

    pub fn identity(&self) -> Result<Identity> {
        let mut id = Identity::load(&self.config.root, &need_identity(&self.config)?)?;
        id.home = self.home.clone();
        id.cwd = self.cwd.clone();
        id.project = self.config.project.clone();
        Ok(id)
    }

    pub fn list(&self) -> Vec<Row> {
        status::list(&self.config.root)
    }

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

    pub fn templates(&self) -> Vec<Template> {
        templates::all()
            .into_iter()
            .map(|(name, file, _)| Template {
                name,
                override_path: templates::override_path(&self.config.root, &file),
            })
            .collect()
    }

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

    pub fn sync_done(&self, req: &SyncDoneRequest) -> Result<Bookmark> {
        req.check()?;
        Ok(sync::done(&self.config.root, &req.session, req.until)?)
    }

    pub fn sync_due(&self) -> Result<Vec<SyncDue>> {
        let slug = match self.config.identity_source {
            Some(Source::Flag) => Some(need_identity(&self.config)?),
            _ => None,
        };
        Ok(sync::due(&self.config.root, slug.as_deref())?)
    }
}

pub struct SessionSync<'a> {
    root: &'a Path,
    cwd: &'a Path,
    slug: String,
    project: Option<String>,
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

    pub fn start(&self, req: &SyncStartRequest) -> Result<SyncRun> {
        req.check()?;
        Ok(sync::start(
            &self.request(&req.session, &req.options),
            &utf8(&req.transcript),
            req.format,
        )?)
    }

    pub fn push(&self, session: &str, turns: &[Turn], opts: &SyncOptions) -> Result<SyncRun> {
        opts.check()?;
        Ok(sync::push(&self.request(session, opts), turns)?)
    }
}

impl Identity {
    pub fn working_project(&self) -> Result<Option<&str>> {
        need_project(&self.dir, self.project.as_deref())
    }

    pub fn lint(&self, req: &LintRequest) -> Result<Vec<Diagnostic>> {
        req.check()?;
        let mut found = lint::run(self, &req.rules);
        if req.changed {
            let paths = changes::changed_paths(self);
            found.retain(|d| paths.contains(&d.path));
        }
        Ok(found)
    }

    pub fn write(&self, req: &WriteRequest) -> Result<bool> {
        req.check()?;
        let text = req.content.as_deref().unwrap_or_default();
        Ok(add::write(self, &req.path, text)?)
    }

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

    pub fn stamp(&self, req: &StampRequest) -> Result<StampReport> {
        req.check()?;
        Ok(stamp::run(self, req.check)?)
    }

    pub fn index(&self, req: &IndexRequest) -> Result<Vec<String>> {
        req.check()?;
        if req.check {
            return Ok(index::stale(self));
        }
        Ok(index::write(self)?)
    }

    pub fn resolve(&self, req: &ResolveRequest) -> Result<Result<Hit, Vec<Hit>>> {
        req.check()?;
        let hits = |v: Vec<usize>| v.iter().map(|i| Hit::of(&self.entries[*i])).collect();
        Ok(match find::resolve(self, &req.name, req.kind) {
            find::Resolution::Found(i) => Ok(Hit::of(&self.entries[i])),
            find::Resolution::Ambiguous(v) | find::Resolution::Missing(v) => Err(hits(v)),
        })
    }

    pub fn show(&self, req: &ShowRequest) -> Result<(String, String)> {
        req.check()?;
        Ok(find::show(self, &req.name, req.section.as_deref())?)
    }

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

    pub fn orphans(&self) -> Vec<Hit> {
        find::orphans(self)
            .into_iter()
            .map(|i| Hit::of(&self.entries[i]))
            .collect()
    }

    pub fn context(&self, req: &ContextRequest) -> Result<Context> {
        req.check()?;
        let project = self.working_project()?;
        Ok(context::build(self, req, project)?)
    }

    pub fn context_text(&self, ctx: &Context) -> String {
        context::render_text(self, ctx)
    }

    pub fn state(&self, req: &StateRequest) -> Result<State> {
        req.check()?;
        let mut s = status::compute(self, req.days, 5);
        if let Some(p) = self.working_project()? {
            s.project = Some(status::project(self, p));
        }
        Ok(s)
    }

    pub fn search(&self, req: &SearchRequest) -> Result<Vec<SearchHit>> {
        req.check()?;
        Ok(search::search(self, req))
    }

    pub fn review(&self) -> Packet {
        review::packet(self)
    }

    pub fn review_done(&self, req: &ReviewDoneRequest) -> Result<ReviewRecord> {
        req.check()?;
        Ok(review::done(self, req.through)?)
    }

    pub fn soul_note(&self, req: &SoulNoteRequest) -> Result<SoulNote> {
        req.check()?;
        Ok(review::soul_note(
            self,
            &req.file,
            &req.change,
            &req.evidence,
        )?)
    }

    pub fn soul(&self, req: &SoulRequest) -> Result<Vec<(String, String)>> {
        req.check()?;
        let named = [req.identity, req.soul, req.voice, req.look, req.memory];
        let asked = if named.contains(&true) {
            named
        } else {
            [true, true, false, false, false]
        };
        let mut files = vec![];
        for (file, _) in identity::SOUL_FILES.iter().zip(asked).filter(|(_, on)| *on) {
            let path = format!("soul/{file}");
            let Some(e) = self.entries.iter().find(|e| e.rel == path) else {
                return Err(anyhow::anyhow!("missing {path}").into());
            };
            files.push((path, e.text.clone()));
        }
        Ok(files)
    }

    pub fn soul_history(&self, req: &SoulHistoryRequest) -> Result<Vec<SoulChange>> {
        req.check()?;
        Ok(review::soul_history(self, req.file.as_deref()))
    }

    pub fn changes(&self, req: &ChangesRequest) -> Result<Changes> {
        req.check()?;
        let since = req
            .since
            .clone()
            .or_else(|| req.days.map(state::local_day_ago));
        Ok(changes::changes(self, since.as_deref())?)
    }

    pub fn export_card(&self, req: &CardRequest) -> Result<Card> {
        req.check()?;
        let project = need_project(&self.dir, req.project.as_deref())?;
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

    pub fn export_skill(&self, req: &SkillRequest) -> Result<String> {
        req.check()?;
        let out = utf8(&req.out);
        export::skill(self, &self.cwd.join(&out))?;
        Ok(out.join("SKILL.md").display().to_string())
    }

    pub fn export_soulspec(&self, req: &SoulspecRequest) -> Result<Soulspec> {
        req.check()?;
        let out = utf8(&req.out);
        Ok(export::soulspec(self, &self.cwd.join(&out), &out)?)
    }
}
