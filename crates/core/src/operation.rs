use crate::request::{Args, DryRun, Field, Request};
use crate::*;
use serde::Serialize;
use serde_json::json;
use std::io::Read;

struct Call<'a> {
    select: &'a Select,
    env: &'a Env,
    input: &'a mut dyn Read,
    warnings: Vec<String>,
}

impl Call<'_> {
    fn root(&mut self) -> Result<Root> {
        let root = Root::open(self.select, self.env)?;
        self.warnings.extend(root.warnings.iter().cloned());
        Ok(root)
    }

    fn identity(&mut self) -> Result<Identity> {
        self.root()?.identity()
    }

    fn input(&mut self) -> Result<String> {
        Ok(std::io::read_to_string(&mut self.input).map_err(anyhow::Error::from)?)
    }
}

type Run = dyn Fn(&Args, &mut Call) -> Result<Result<Outcome>> + Send + Sync;

pub struct Operation {
    pub path: &'static [&'static str],
    pub about: &'static str,
    pub fields: Vec<Field>,
    run: Box<Run>,
}

pub struct Ran {
    pub warnings: Vec<String>,
    pub result: Result<Outcome>,
}

impl Operation {
    pub fn name(&self) -> String {
        self.path.join("_").replace('-', "_")
    }

    pub fn run(
        &self,
        args: &Args,
        select: &Select,
        env: &Env,
        input: &mut dyn Read,
    ) -> Result<Ran> {
        let mut s = Call {
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

fn op<R: Request + 'static>(
    path: &'static [&'static str],
    about: &'static str,
    call: impl Fn(R, &mut Call) -> Result<Outcome> + Send + Sync + 'static,
) -> Operation {
    Operation {
        path,
        about,
        fields: R::fields(),
        run: Box::new(move |args, s| Ok(call(R::from_args(args)?, s))),
    }
}

fn write_op<R: Request + 'static>(
    path: &'static [&'static str],
    about: &'static str,
    call: fn(R, &mut Call) -> Result<Outcome>,
) -> Operation {
    op::<DryRun<R>>(path, about, move |DryRun { req, dry_run }, s| {
        if !dry_run {
            return call(req, s);
        }
        let (result, writes) = crate::write::dry_run(|| call(req, s));
        result.map(|outcome| Outcome::DryRun {
            outcome: Box::new(outcome),
            writes,
        })
    })
}

fn templates(_: (), s: &mut Call) -> Result<Outcome> {
    Ok(Outcome::Templates(s.root()?.templates()))
}

fn push_turns(checked: &SessionSync, r: SyncPushRequest, s: &mut Call) -> Result<Outcome> {
    let text = r.turns.map_or_else(|| s.input(), Ok)?;
    let turns = parse_turns(&text)?;
    Ok(Outcome::Synced(
        checked.push(&r.session, &turns, &r.options)?,
    ))
}

pub fn operations() -> Vec<Operation> {
    vec![
        write_op::<NewRequest>(
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
        write_op::<WriteRequest>(
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
        write_op::<AddRequest>(
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
        write_op::<StampRequest>(
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
                found: id.lint(&r)?,
                identity: id.slug,
            })
        }),
        write_op::<IndexRequest>(
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
            templates,
        ),
        op::<()>(
            &["templates", "list"],
            "List template names and where each comes from",
            templates,
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
                let found = s.identity()?.resolve(&r)?;
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
            "The tiered load set: core files, requested tiers, and the generated Now block last",
            |r, s| {
                let id = s.identity()?;
                let context = id.context(&r)?;
                let text = if r.format == ContextFormat::Paths {
                    context.paths_text()
                } else {
                    id.context_text(&context)
                };
                Ok(Outcome::Context { context, text })
            },
        ),
        op::<StateRequest>(
            &["state"],
            "Derived current state: activity, recent days and events, open threads, reflection",
            |r, s| Ok(Outcome::State(s.identity()?.state(&r)?)),
        ),
        op::<SearchRequest>(
            &["search"],
            "BM25 search over titles, aliases, summaries and bodies",
            |mut r, s| {
                r.project = s.select.project.clone();
                Ok(Outcome::Search(s.identity()?.search(&r)?))
            },
        ),
        op::<()>(
            &["reflect"],
            "Print the reflection digest, or record a finished reflection with `done`",
            |_, s| Ok(Outcome::Reflect(s.identity()?.reflect())),
        ),
        write_op::<ReflectDoneRequest>(
            &["reflect", "done"],
            "Record a finished reflection (dates and the MEMORY.md hash)",
            |r, s| Ok(Outcome::ReflectDone(s.identity()?.reflect_done(&r)?)),
        ),
        op::<PersonaRequest>(
            &["persona"],
            "Print persona/IDENTITY.md and persona/SOUL.md, or only the persona files asked for; `change` and `changelog` keep the persona changelog",
            |r, s| {
                let files = s.identity()?.persona(&r)?;
                let text = files
                    .iter()
                    .map(|(path, text)| format!("<!-- owo: {path} -->\n{}\n", text.trim_end()))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(Outcome::Persona { files, text })
            },
        ),
        write_op::<PersonaChangeRequest>(
            &["persona", "change"],
            "Record a persona change with its evidence",
            |r, s| Ok(Outcome::PersonaChange(s.identity()?.persona_change(&r)?)),
        ),
        op::<PersonaChangelogRequest>(
            &["persona", "changelog"],
            "Print the persona changelog, optionally for one file",
            |r, s| Ok(Outcome::PersonaChangelog(s.identity()?.persona_changelog(&r)?)),
        ),
        op::<ChangesRequest>(
            &["changes"],
            "What changed, grouped by day (Git history, else state stamps)",
            |r, s| Ok(Outcome::Changes(s.identity()?.changes(&r)?)),
        ),
        write_op::<PersonaCardRequest>(
            &["export", "persona-card"],
            "Persona Card JSON in the Character Card V3 format, with V2-compatible fields, or V2",
            |mut r, s| {
                r.project = s.select.project.clone();
                let card = s.identity()?.export_persona_card(&r)?;
                Ok(Outcome::PersonaCard { card, out: r.out })
            },
        ),
        write_op::<SkillRequest>(
            &["export", "skill"],
            "SKILL.md wrapper that loads the identity with owo",
            |r, s| Ok(Outcome::Skill(s.identity()?.export_skill(&r)?)),
        ),
        write_op::<SoulspecRequest>(
            &["export", "soulspec"],
            "SoulSpec v0.5 package folder",
            |r, s| {
                let report = s.identity()?.export_soulspec(&r)?;
                Ok(Outcome::Soulspec { dir: r.out, report })
            },
        ),
        write_op::<SyncStartRequest>(
            &["sync", "start"],
            "Read the transcript and, when a run is due, write the slice and the ingest prompt",
            |r, s| Ok(Outcome::Synced(s.root()?.sync()?.start(&r)?)),
        ),
        write_op::<SyncPushRequest>(
            &["sync", "push"],
            "Store turns sent by the host, then run sync start on them (for hosts without a local transcript)",
            |r, s| {
                let root = s.root()?;
                push_turns(&root.sync()?, r, s)
            },
        ),
        write_op::<SyncDoneRequest>(
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
        op::<()>(
            &["sync", "due"],
            "List the sessions in the root that have a pending run",
            |_, s| Ok(Outcome::SyncDue(s.root()?.sync_due()?)),
        ),
        write_op::<MigrateRequest>(
            &["migrate", "v1"],
            "v1 identity folder (with references/) into <out> = <root>/<slug>",
            |r, s| Ok(Outcome::Migrate(migrate_v1(&r, &s.env.cwd)?)),
        ),
        op::<()>(
            &["config"],
            "Show how root, identity and project resolve",
            |_, s| Ok(Outcome::Config(s.root()?.config)),
        ),
    ]
}

pub enum Outcome {
    Slug(String),
    Config(Resolved),
    Created(Created),
    List {
        root: PathBuf,
        rows: Vec<ListedIdentity>,
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
    Index {
        check: bool,
        paths: Vec<String>,
    },
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
    Context {
        context: Context,
        text: String,
    },
    State(CurrentState),
    Search(Vec<SearchHit>),
    Synced(SyncReport),
    SyncDone {
        session: String,
        until: usize,
        bookmark: Bookmark,
    },
    SyncDue(Vec<PendingSession>),
    Migrate(MigrateReport),
    Reflect(ReflectionDigest),
    ReflectDone(ReflectionRecord),
    Persona {
        files: Vec<(String, String)>,
        text: String,
    },
    PersonaChange(PersonaChangeReport),
    PersonaChangelog(Vec<PersonaChange>),
    Changes(Changes),
    PersonaCard {
        card: PersonaCard,
        out: Option<PathBuf>,
    },
    Skill(String),
    Soulspec {
        dir: PathBuf,
        report: SoulspecReport,
    },
    Templates(Vec<Template>),
    Template {
        template: Template,
        text: String,
    },
    DryRun {
        outcome: Box<Outcome>,
        writes: Vec<String>,
    },
}

#[derive(Debug, Default, Serialize)]
pub struct Envelope {
    pub ok: bool,
    pub data: Value,
    pub warnings: Vec<Value>,
    pub errors: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub would_write: Option<Vec<String>>,
}

impl Envelope {
    pub fn new(ok: bool, data: Value) -> Self {
        Self {
            ok,
            data,
            ..Self::default()
        }
    }

    pub fn ok(data: Value) -> Self {
        Self::new(true, data)
    }

    pub fn fail(message: &str) -> Self {
        Self::new(false, Value::Null).with_errors([message])
    }

    pub fn with_warnings(mut self, items: impl IntoIterator<Item = impl Into<Value>>) -> Self {
        self.warnings.extend(items.into_iter().map(Into::into));
        self
    }

    pub fn text_warnings(&self) -> impl Iterator<Item = String> + '_ {
        self.warnings
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
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
                let ok = *created || *kind == Kind::DailyLog;
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
            Outcome::SyncDue(list) => Envelope::ok(json!({ "due": list })),
            Outcome::Migrate(r) => Envelope::ok(value(r)),
            Outcome::Reflect(p) => Envelope::ok(value(p)),
            Outcome::ReflectDone(r) => Envelope::ok(value(r)),
            Outcome::Persona { files, .. } => {
                let files: Vec<Value> = files
                    .iter()
                    .map(|(path, text)| json!({ "path": path, "text": text }))
                    .collect();
                Envelope::ok(json!({ "files": files }))
            }
            Outcome::PersonaChange(change) => {
                Envelope::ok(value(&change.record)).with_warnings(change.warning.clone())
            }
            Outcome::PersonaChangelog(list) => Envelope::ok(json!({ "changelog": list })),
            Outcome::Changes(c) => Envelope::ok(value(c)),
            Outcome::PersonaCard { card, out } => {
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
            Outcome::DryRun { outcome, writes } => Envelope {
                would_write: Some(writes.clone()),
                ..outcome.envelope()
            },
        }
    }
}
