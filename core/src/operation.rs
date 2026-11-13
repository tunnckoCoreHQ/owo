//! Every operation by name, with its request type: `operations` is the list the CLI builds its
//! commands from and the MCP server its tools. `Operation::run` maps checked arguments into the
//! request, opens the root, makes the typed call, which checks the rules, and returns
//! the typed `Outcome`. `Outcome::envelope` is the `{ ok, data, warnings, errors }` JSON of a
//! result, which the CLI prints with `--json` and the MCP server returns.

use crate::request::{Args, Field, Request};
use crate::*;
use serde::Serialize;
use serde_json::json;
use std::io::Read;

/// What one operation call reads beside its request: the root it opens, the caller's input,
/// and the warnings found on the way.
struct Session<'a> {
    select: &'a Select,
    env: &'a Env,
    input: &'a mut dyn Read,
    warnings: Vec<String>,
}

impl Session<'_> {
    /// The selected root; its config warnings join the warnings.
    fn root(&mut self) -> Result<Root> {
        let root = Root::open(self.select, self.env)?;
        self.warnings.extend(root.warnings.iter().cloned());
        Ok(root)
    }

    fn identity(&mut self) -> Result<Identity> {
        self.root()?.identity()
    }

    /// The whole caller input, read once, when a request leaves out a text it can take from it.
    fn input(&mut self) -> Result<String> {
        Ok(std::io::read_to_string(&mut self.input).map_err(anyhow::Error::from)?)
    }
}

type Run = dyn Fn(&Args, &mut Session) -> Result<Result<Outcome>, String> + Send + Sync;

/// One operation: its name, its help line, the fields of its request type, and its call.
pub struct Operation {
    /// The words of its name: `["review", "done"]`.
    pub path: &'static [&'static str],
    pub about: &'static str,
    pub fields: Vec<Field>,
    run: Box<Run>,
}

/// What `Operation::run` gives: the warnings found before the result or the failure, and the
/// result.
pub struct Ran {
    pub warnings: Vec<String>,
    pub result: Result<Outcome>,
}

impl Operation {
    /// The name of the operation as one word: `review_done`.
    pub fn name(&self) -> String {
        self.path.join("_")
    }

    /// Runs the operation with `args`, checked with `Args::new`. `select` and `env` pick the
    /// root, and `input` gives the text that `write` and `sync push` read when the request has
    /// none, only after the root resolves. An argument of the wrong type is the error text,
    /// before any call.
    pub fn run(
        &self,
        args: &Args,
        select: &Select,
        env: &Env,
        input: &mut dyn Read,
    ) -> Result<Ran, String> {
        let mut s = Session {
            select,
            env,
            input,
            warnings: vec![],
        };
        let result = (self.run)(args, &mut s)?;
        Ok(Ran {
            warnings: s.warnings,
            result,
        })
    }
}

/// The operation `path`, with the request type `R` and its call.
fn op<R: Request + 'static>(
    path: &'static [&'static str],
    about: &'static str,
    call: fn(R, &mut Session) -> Result<Outcome>,
) -> Operation {
    let fields = R::fields();
    Operation {
        path,
        about,
        fields,
        run: Box::new(move |args, s| {
            let req = R::from_args(args)?;
            Ok(call(req, s))
        }),
    }
}

/// Every operation, in the order the CLI lists them.
pub fn operations() -> Vec<Operation> {
    vec![
        op::<NewRequest>(
            &["new"],
            "Create a new identity from the built-in templates",
            |r, s| Ok(Outcome::Created(s.root()?.create(&r)?)),
        ),
        op::<()>(&["list"], "List identities in the root", |_, s| {
            let root = s.root()?;
            Ok(Outcome::List {
                root: root.path().to_path_buf(),
                rows: root.list(),
            })
        }),
        op::<WriteRequest>(
            &["write"],
            "Write the full text of one identity file (create or replace)",
            |mut r, s| {
                let id = s.identity()?;
                if r.content.is_none() {
                    r.content = Some(s.input()?);
                }
                let created = id.write(&r)?;
                Ok(Outcome::Write {
                    path: r.path,
                    created,
                })
            },
        ),
        op::<AddRequest>(
            &["add"],
            "Create an entry from its template at its deterministic path",
            |r, s| {
                let (path, created) = s.identity()?.add(&r)?;
                Ok(Outcome::Add {
                    kind: r.kind,
                    path,
                    created,
                })
            },
        ),
        op::<StampRequest>(
            &["stamp"],
            "Refresh state.json files from content hashes",
            |r, s| {
                Ok(Outcome::Stamp {
                    check: r.check,
                    report: s.identity()?.stamp(&r)?,
                })
            },
        ),
        op::<LintRequest>(&["lint"], "Validate an identity", |r, s| {
            let id = s.identity()?;
            Ok(Outcome::Lint {
                found: id.lint(&r),
                identity: id.slug,
            })
        }),
        op::<IndexRequest>(
            &["index"],
            "Regenerate _index.md files and life/timeline.md",
            |r, s| {
                Ok(Outcome::Index {
                    check: r.check,
                    paths: s.identity()?.index(&r)?,
                })
            },
        ),
        op::<SlugRequest>(&["slug"], "Print the slug for a text", |r, _| {
            Ok(Outcome::Slug(slug(&r)?))
        }),
        op::<()>(
            &["templates"],
            "List templates, or show one (built in, or overridden in <root>/.templates/)",
            |_, s| Ok(Outcome::Templates(s.root()?.templates())),
        ),
        op::<()>(
            &["templates", "list"],
            "List template names and where each comes from",
            |_, s| Ok(Outcome::Templates(s.root()?.templates())),
        ),
        op::<TemplateRequest>(
            &["templates", "show"],
            "Print a template as `owo new` and `owo add` would use it",
            |r, s| {
                let (template, text) = s.root()?.template(&r)?;
                Ok(Outcome::Template { template, text })
            },
        ),
        op::<ResolveRequest>(
            &["resolve"],
            "Resolve a name to an entry path the way links do",
            |r, s| {
                let found = s.identity()?.resolve(&r);
                Ok(Outcome::Resolve {
                    name: r.name,
                    found,
                })
            },
        ),
        op::<ShowRequest>(
            &["show"],
            "Print an entry, or one section of it",
            |r, s| {
                let (path, text) = s.identity()?.show(&r)?;
                Ok(Outcome::Show { path, text })
            },
        ),
        op::<LinksRequest>(
            &["links"],
            "Backlinks and outlinks of an entry, or every orphan",
            |r, s| {
                let id = s.identity()?;
                if r.orphans {
                    return Ok(Outcome::Orphans(id.orphans()));
                }
                Ok(Outcome::Links(id.links(&r)?))
            },
        ),
        op::<ContextRequest>(
            &["context"],
            "The tiered load set: core files, the generated Now block, and requested tiers",
            |r, s| {
                let id = s.identity()?;
                let context = id.context(&r)?;
                let text = if r.paths {
                    context.paths_text()
                } else {
                    id.context_text(&context)
                };
                Ok(Outcome::Context { context, text })
            },
        ),
        op::<StateRequest>(
            &["state"],
            "Derived current state: activity, recent days and events, open threads, review",
            |r, s| Ok(Outcome::State(s.identity()?.state(&r)?)),
        ),
        op::<SearchRequest>(
            &["search"],
            "BM25 search over titles, aliases, summaries and bodies",
            |mut r, s| {
                // The project filter is the selected project, not the configured one.
                r.project = s.select.project.clone();
                Ok(Outcome::Search(s.identity()?.search(&r)))
            },
        ),
        op::<()>(
            &["review"],
            "Print the review packet, or record a finished review with `done`",
            |_, s| Ok(Outcome::Review(s.identity()?.review())),
        ),
        op::<ReviewDoneRequest>(
            &["review", "done"],
            "Record a finished review (dates and the MEMORY.md hash)",
            |r, s| Ok(Outcome::ReviewDone(s.identity()?.review_done(&r)?)),
        ),
        op::<SoulNoteRequest>(
            &["soul", "note"],
            "Append a soul change with its evidence",
            |r, s| Ok(Outcome::SoulNote(s.identity()?.soul_note(&r)?)),
        ),
        op::<SoulHistoryRequest>(
            &["soul", "history"],
            "Print the soul history, optionally for one file",
            |r, s| Ok(Outcome::SoulHistory(s.identity()?.soul_history(&r))),
        ),
        op::<ChangesRequest>(
            &["changes"],
            "What changed, grouped by day (Git history, else state stamps)",
            |r, s| Ok(Outcome::Changes(s.identity()?.changes(&r)?)),
        ),
        op::<CardRequest>(
            &["export", "card"],
            "Character Card JSON (V3 with V2-compatible fields, or V2)",
            |mut r, s| {
                // The project filter is the selected project, not the configured one.
                r.project = s.select.project.clone();
                let card = s.identity()?.export_card(&r)?;
                Ok(Outcome::Card { card, out: r.out })
            },
        ),
        op::<SkillRequest>(
            &["export", "skill"],
            "SKILL.md wrapper that loads the identity with owo",
            |r, s| Ok(Outcome::Skill(s.identity()?.export_skill(&r)?)),
        ),
        op::<SoulspecRequest>(
            &["export", "soulspec"],
            "SoulSpec v0.5 package folder",
            |r, s| {
                let report = s.identity()?.export_soulspec(&r)?;
                Ok(Outcome::Soulspec { dir: r.out, report })
            },
        ),
        op::<SyncStartRequest>(
            &["sync", "start"],
            "Read the transcript and, when a run is due, write the slice and the ingest prompt",
            |r, s| Ok(Outcome::Synced(s.root()?.sync()?.start(&r)?)),
        ),
        op::<SyncPushRequest>(
            &["sync", "push"],
            "Store turns sent by the host, then run sync start on them (for hosts without a local transcript)",
            |r, s| {
                let root = s.root()?;
                let sync = root.sync()?;
                // The input is read after the session checks of `sync`.
                let turns = match r.turns {
                    Some(t) => t,
                    None => s.input()?,
                };
                // The turns parse before the call, so a bad turns text fails before the
                // checks of the session.
                let turns = parse_turns(&turns)?;
                Ok(Outcome::Synced(sync.push(&r.session, &turns, &r.options)?))
            },
        ),
        op::<SyncDoneRequest>(
            &["sync", "done"],
            "Move the bookmark after a successful ingest run",
            |r, s| {
                let bookmark = s.root()?.sync_done(&r)?;
                Ok(Outcome::SyncDone {
                    session: r.session,
                    until: r.until,
                    bookmark,
                })
            },
        ),
        op::<MigrateRequest>(
            &["migrate", "v1"],
            "v1 identity folder (with references/) into <out> = <root>/<slug>",
            |r, _| Ok(Outcome::Migrate(migrate_v1(&r)?)),
        ),
        op::<()>(
            &["config"],
            "Show how root, identity and project resolve",
            |_, s| Ok(Outcome::Config(s.root()?.config)),
        ),
    ]
}

/// The typed result of one operation, with what its output needs beside the library result.
pub enum Outcome {
    Slug(String),
    Config(Resolved),
    Created(Created),
    List {
        root: PathBuf,
        rows: Vec<Row>,
    },
    Lint {
        identity: String,
        found: Vec<Diagnostic>,
    },
    Write {
        path: String,
        created: bool,
    },
    Add {
        kind: Kind,
        path: String,
        created: bool,
    },
    Stamp {
        check: bool,
        report: StampReport,
    },
    /// The written generated files, or with `check` the stale ones.
    Index {
        check: bool,
        paths: Vec<String>,
    },
    /// The entry, or the candidates when `name` does not resolve to one.
    Resolve {
        name: String,
        found: Result<Hit, Vec<Hit>>,
    },
    Show {
        path: String,
        text: String,
    },
    Orphans(Vec<Hit>),
    Links(Links),
    /// The load set and its text: the file contents, or the file list.
    Context {
        context: Context,
        text: String,
    },
    State(State),
    Search(Vec<SearchHit>),
    /// A `sync start` or `sync push` run.
    Synced(SyncRun),
    SyncDone {
        session: String,
        until: usize,
        bookmark: Bookmark,
    },
    Migrate(MigrateReport),
    Review(Packet),
    ReviewDone(ReviewRecord),
    SoulNote(SoulNote),
    SoulHistory(Vec<SoulChange>),
    Changes(Changes),
    /// A built card, and the file it was written to.
    Card {
        card: Card,
        out: Option<PathBuf>,
    },
    Skill(String),
    Soulspec {
        dir: PathBuf,
        report: Soulspec,
    },
    Templates(Vec<Template>),
    Template {
        template: Template,
        text: String,
    },
}

/// The JSON shape of every result: `{ ok, data, warnings, errors }`.
#[derive(Debug, Serialize)]
pub struct Envelope {
    pub ok: bool,
    pub data: Value,
    pub warnings: Vec<Value>,
    pub errors: Vec<Value>,
}

impl Envelope {
    pub fn new(ok: bool, data: Value) -> Self {
        Self {
            ok,
            data,
            warnings: vec![],
            errors: vec![],
        }
    }

    pub fn ok(data: Value) -> Self {
        Self::new(true, data)
    }

    /// A failure with one error text.
    pub fn fail(message: &str) -> Self {
        Self::new(false, Value::Null).with_errors([message])
    }

    pub fn with_warnings(mut self, items: impl IntoIterator<Item = impl Into<Value>>) -> Self {
        self.warnings.extend(items.into_iter().map(Into::into));
        self
    }

    pub fn with_errors(mut self, items: impl IntoIterator<Item = impl Into<Value>>) -> Self {
        self.errors.extend(items.into_iter().map(Into::into));
        self
    }
}

fn value(v: impl Serialize) -> Value {
    serde_json::to_value(v).expect("a result serializes")
}

impl Outcome {
    /// The envelope of the result. `ok` is false when the operation found a problem, such as
    /// lint errors or a stale check.
    pub fn envelope(&self) -> Envelope {
        match self {
            Outcome::Slug(slug) => Envelope::ok(json!({ "slug": slug })),
            Outcome::Config(r) => Envelope::ok(value(r)),
            Outcome::Created(c) => {
                Envelope::ok(json!({ "slug": c.slug, "path": c.dir, "files": c.files }))
            }
            Outcome::List { root, rows } => {
                Envelope::ok(json!({ "root": root, "identities": rows }))
            }
            Outcome::Lint { identity, found } => {
                let split = |level: Level| {
                    found
                        .iter()
                        .filter(|d| d.level == level)
                        .map(value)
                        .collect::<Vec<_>>()
                };
                let errors = split(Level::Error);
                Envelope::new(
                    errors.is_empty(),
                    json!({ "identity": identity, "infos": split(Level::Info) }),
                )
                .with_warnings(split(Level::Warning))
                .with_errors(errors)
            }
            Outcome::Write { path, created } => {
                Envelope::ok(json!({ "path": path, "created": created }))
            }
            Outcome::Add {
                kind,
                path,
                created,
            } => {
                let ok = *created || *kind == Kind::Log;
                Envelope::new(ok, json!({ "path": path, "created": created }))
                    .with_errors((!ok).then(|| format!("exists: {path}")))
            }
            Outcome::Stamp { check, report } => {
                Envelope::new(!check || report.is_clean(), value(report))
            }
            Outcome::Index { check, paths } => {
                let key = if *check { "stale" } else { "written" };
                Envelope::new(!check || paths.is_empty(), json!({ key: paths }))
            }
            Outcome::Resolve { name, found } => match found {
                Ok(hit) => Envelope::ok(value(hit)),
                Err(hits) => Envelope::new(false, json!({ "candidates": hits }))
                    .with_errors([crate::find::unresolved(name)]),
            },
            Outcome::Show { path, text } => Envelope::ok(json!({ "path": path, "text": text })),
            Outcome::Orphans(hits) => Envelope::ok(json!({ "orphans": hits })),
            Outcome::Links(l) => {
                Envelope::ok(json!({ "path": l.path, "in": l.inbound, "out": l.outbound }))
            }
            Outcome::Context { context, .. } => {
                Envelope::ok(value(context)).with_warnings(context.warnings.clone())
            }
            Outcome::State(s) => Envelope::ok(value(s)),
            Outcome::Search(hits) => {
                let ok = !hits.is_empty();
                Envelope::new(ok, json!({ "hits": hits })).with_errors((!ok).then_some("no hits"))
            }
            Outcome::Synced(s) => Envelope::ok(value(s)),
            Outcome::SyncDone { bookmark, .. } => Envelope::ok(value(bookmark)),
            Outcome::Migrate(r) => Envelope::ok(value(r)),
            Outcome::Review(p) => Envelope::ok(value(p)),
            Outcome::ReviewDone(r) => Envelope::ok(value(r)),
            Outcome::SoulNote(note) => {
                Envelope::ok(value(&note.record)).with_warnings(note.warning.clone())
            }
            Outcome::SoulHistory(list) => Envelope::ok(json!({ "history": list })),
            Outcome::Changes(c) => Envelope::ok(value(c)),
            Outcome::Card { card, out } => {
                let data = match out {
                    Some(f) => json!({ "path": f }),
                    None => card.card.clone(),
                };
                Envelope::ok(data).with_warnings(card.warnings.clone())
            }
            Outcome::Skill(path) => Envelope::ok(json!({ "path": path })),
            Outcome::Soulspec { dir, report } => {
                Envelope::ok(json!({ "dir": dir, "files": report.files }))
                    .with_warnings(report.warnings.clone())
            }
            Outcome::Templates(list) => Envelope::ok(json!({ "templates": list })),
            Outcome::Template { template, text } => Envelope::ok(
                json!({ "name": template.name, "override": template.override_path, "text": text }),
            ),
        }
    }
}
