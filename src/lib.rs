//! owo: one world, one identity. The library core for living agent identities.
//!
//! `run` takes a parsed command, the environment values and a stdin reader, and returns an
//! `Output`; it prints nothing. `mcp` serves every command as an MCP tool and `acp` runs the
//! ACP proxy, on streams the caller gives them. The library reads nothing from the process:
//! the `owo` binary passes in its arguments, environment and streams.

pub mod acp;
mod add;
mod changes;
mod cli;
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
pub mod mcp;
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

pub use cli::{
    Cli, Command, ExportFormat, MigrateFrom, ReviewAction, SoulAction, SyncAction, TemplatesAction,
};
pub use identity::Identity;
pub use output::Output;

use anyhow::Result;
use clap::CommandFactory;
use config::{need_identity, need_project, ConfigError};
use identity::Kind;
use output::{Envelope, Level, Out};
use serde_json::json;
use std::io::Read;
use std::path::PathBuf;

/// What a command reads from its process: environment values and the working folder. The
/// caller reads them once; `run`, `mcp` and `acp` only use them.
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

/// Runs one command and returns what it produced. `stdin` gives the text for `owo write` and
/// `owo sync push` when they take no `--content` or `--turns`; `run` reads it only when the
/// command needs the text. `Command::Mcp` is a server and `Command::Acp` a proxy, not
/// commands, so they return an error here.
pub fn run(cli: &Cli, env: &Env, stdin: &mut dyn Read) -> Output {
    let mut out = Out {
        json: cli.json,
        quiet: cli.quiet,
        warnings: vec![],
    };
    match dispatch(cli, env, stdin, &mut out) {
        Ok(o) => o,
        Err(e) => Output {
            warnings: out.warnings,
            ..Output::from(e)
        },
    }
}

/// The global `--root`, `-i` and `--project` flags of a command line that runs in process.
fn global_args(cli: &Cli, identity: Option<String>, project: Option<String>) -> Vec<String> {
    let mut out = vec![];
    if let Some(root) = &cli.root {
        out.extend(["--root".into(), root.display().to_string()]);
    }
    if let Some(i) = identity {
        out.extend(["-i".into(), i]);
    }
    if let Some(p) = project {
        out.extend(["--project".into(), p]);
    }
    out
}

/// Parses `argv` (without the program name) with clap and calls `run` in process, with
/// `stdin` as the stdin text.
fn run_argv(argv: Vec<String>, env: &Env, stdin: &str) -> Result<(Cli, Output), clap::Error> {
    let cli: Cli = clap::Parser::try_parse_from(std::iter::once("owo".into()).chain(argv))?;
    let out = run(&cli, env, &mut stdin.as_bytes());
    Ok((cli, out))
}

fn resolved(cli: &Cli, env: &Env, out: &mut Out) -> Result<config::Resolved> {
    let inputs = config::Inputs::new(cli, env);
    let r = config::resolve(&inputs).map_err(|e| ConfigError(format!("{e:#}")))?;
    if let config::Source::ProjectFile(file) = &r.root_source {
        if file.parent() != Some(inputs.cwd.as_path()) {
            out.warnings.push(format!(
                "the root comes from {} in a parent folder",
                file.display()
            ));
        }
    }
    Ok(r)
}

fn load(cli: &Cli, env: &Env, out: &mut Out) -> Result<(config::Resolved, Identity)> {
    let r = resolved(cli, env, out)?;
    let mut id = Identity::load(&r.root, &need_identity(&r)?)?;
    id.home = env.home.clone();
    Ok((r, id))
}

/// Runs `sync start` or `sync push` with the shared identity, project and run size.
fn sync_cmd(
    cli: &Cli,
    env: &Env,
    out: &mut Out,
    session: &str,
    now: bool,
    every: usize,
    step: impl FnOnce(&sync::StartRequest) -> Result<sync::Start>,
) -> Result<Output> {
    let r = resolved(cli, env, out)?;
    let slug = need_identity(&r)?;
    let dir = r.root.join(&slug);
    let project = need_project(&dir, r.project.as_deref())?;
    let every = every.max(1);
    let s = step(&sync::StartRequest {
        root: &r.root,
        slug: &slug,
        session,
        now,
        every,
        project,
    })?;
    out.emit(Envelope::ok(serde_json::to_value(&s)?), |t| {
        if let Some(n) = s.added {
            t.say(format!("added {n}"));
        }
        if s.source.is_some() {
            t.line(sync::render(&s));
        } else {
            t.say(sync::render(&s));
        }
    })
}

fn dispatch(cli: &Cli, env: &Env, stdin: &mut dyn Read, out: &mut Out) -> Result<Output> {
    match &cli.command {
        Command::Slug { text } => {
            let value = slug::slugify(&text.join(" "));
            if value.is_empty() {
                anyhow::bail!(
                    "{:?} has no letters or digits to make a slug",
                    text.join(" ")
                );
            }
            out.emit(Envelope::ok(json!({ "slug": value })), |t| t.line(&value))
        }
        Command::Templates { action } => templates_cmd(cli, action.as_ref(), env, out),
        Command::Mcp { .. } => anyhow::bail!("`owo mcp` is a server; serve it with `owo::mcp`"),
        Command::Acp { .. } => anyhow::bail!("`owo acp` is a proxy; run it with `owo::acp`"),
        Command::Config => {
            let r = resolved(cli, env, out)?;
            out.emit(Envelope::ok(serde_json::to_value(&r)?), |t| {
                t.line(format!("root: {} ({:?})", r.root.display(), r.root_source));
                t.line(format!(
                    "identity: {}",
                    r.identity.as_deref().unwrap_or("-")
                ));
                t.line(format!("project: {}", r.project.as_deref().unwrap_or("-")));
            })
        }
        Command::New {
            name,
            slug: slug_flag,
        } => {
            let r = resolved(cli, env, out)?;
            let slug_value = slug_flag.clone().unwrap_or_else(|| slug::slugify(name));
            let c = create::new_identity(&r.root, name.trim(), &slug_value)?;
            let data = json!({ "slug": c.slug, "path": c.dir, "files": c.files });
            out.emit(Envelope::ok(data), |t| {
                t.say(format!(
                    "created {} ({} files)",
                    c.dir.display(),
                    c.files.len()
                ));
                for f in &c.files {
                    t.say(format!("  {f}"));
                }
            })
        }
        Command::List => {
            let r = resolved(cli, env, out)?;
            let rows = status::list(&r.root);
            out.emit(
                Envelope::ok(json!({ "root": r.root, "identities": rows })),
                |t| t.push(status::render_list(&rows)),
            )
        }
        Command::Lint { rules, changed } => {
            let (_, id) = load(cli, env, out)?;
            let mut found = lint::run(&id, rules);
            if *changed {
                let paths = changes::changed_paths(&id);
                found.retain(|d| paths.contains(&d.path));
            }
            let count = |level: Level| found.iter().filter(|d| d.level == level).count();
            let (errors, warnings) = (count(Level::Error), count(Level::Warning));
            let split = |level: Level| {
                found
                    .iter()
                    .filter(|d| d.level == level)
                    .map(|d| serde_json::to_value(d).expect("serializes"))
                    .collect::<Vec<_>>()
            };
            let slug = &id.slug;
            let envelope = Envelope::new(
                errors == 0,
                json!({ "identity": slug, "infos": split(Level::Info) }),
            )
            .with_warnings(split(Level::Warning))
            .with_errors(split(Level::Error));
            out.emit(envelope, |t| {
                for d in found
                    .iter()
                    .filter(|d| !cli.quiet || d.level == Level::Error)
                {
                    t.line(d.render());
                }
                t.say(format!("{slug}: {errors} errors, {warnings} warnings"));
            })
        }
        Command::Write { path, content } => {
            let (_, id) = load(cli, env, out)?;
            let text = match content {
                Some(c) => c.clone(),
                None => std::io::read_to_string(stdin)?,
            };
            let created = add::write(&id, path, &text)?;
            out.emit(
                Envelope::ok(json!({ "path": path, "created": created })),
                |t| t.say(path),
            )
        }
        Command::Add {
            kind,
            title,
            date,
            parent,
            aliases,
            kb,
        } => {
            let (r, id) = load(cli, env, out)?;
            let title = title.join(" ");
            if title.trim().is_empty() && *kind != Kind::Log {
                return Err(Cli::command()
                    .error(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        format!("`owo add {}` needs a title", kind.name()),
                    )
                    .into());
            }
            // Only a note lives under a project; other types ignore the working project.
            let project = if *kind == Kind::Note {
                need_project(&id.dir, r.project.as_deref())?
            } else {
                None
            };
            let req = add::Request {
                kind: *kind,
                title: &title,
                date: *date,
                parent: parent.as_deref(),
                project,
                aliases,
                kb: kb.as_deref(),
            };
            let (path, created) = add::add(&id, &req)?;
            let ok = created || *kind == Kind::Log;
            let envelope = Envelope::new(ok, json!({ "path": path, "created": created }))
                .with_errors((!ok).then(|| format!("exists: {path}")));
            out.emit(envelope, |t| {
                if created {
                    t.say(&path);
                } else if ok {
                    t.say(format!("exists {path}"));
                } else {
                    t.line(format!("exists {path}"));
                }
            })
        }
        Command::Stamp { check } => {
            let (_, id) = load(cli, env, out)?;
            let report = stamp::run(&id, *check)?;
            let clean = report.is_clean();
            let envelope = Envelope::new(!*check || clean, serde_json::to_value(&report)?);
            out.emit(envelope, |t| {
                for (label, list) in [
                    ("added", &report.added),
                    ("changed", &report.changed),
                    ("removed", &report.removed),
                ] {
                    for p in list {
                        t.say(format!("{label} {p}"));
                    }
                }
                if clean {
                    t.say("state is up to date");
                }
            })
        }
        Command::Index { check } => {
            let (_, id) = load(cli, env, out)?;
            let paths = if *check {
                index::stale(&id)
            } else {
                index::write(&id)?
            };
            let (key, label) = if *check {
                ("stale", "stale ")
            } else {
                ("written", "wrote ")
            };
            let envelope = Envelope::new(!*check || paths.is_empty(), json!({ key: paths }));
            out.emit(envelope, |t| {
                for p in &paths {
                    t.say(format!("{label}{p}"));
                }
                if paths.is_empty() {
                    t.say("generated files are up to date");
                }
            })
        }
        Command::Resolve { name, kind } => {
            let (_, id) = load(cli, env, out)?;
            let name = name.join(" ");
            match find::resolve(&id, &name, kind.as_deref()) {
                find::Resolution::Found(i) => {
                    let hit = find::Hit::of(&id.entries[i]);
                    out.emit(Envelope::ok(serde_json::to_value(&hit)?), |t| {
                        t.line(&hit.path)
                    })
                }
                find::Resolution::Ambiguous(v) | find::Resolution::Missing(v) => {
                    let msg = find::unresolved(&name);
                    let hits: Vec<_> = v.iter().map(|i| find::Hit::of(&id.entries[*i])).collect();
                    let envelope = Envelope::new(false, json!({ "candidates": hits }))
                        .with_errors([msg.as_str()]);
                    let lines: String =
                        hits.iter().map(|h| format!("\n  {}", h.render())).collect();
                    let mut o = out.emit(envelope, |_| {})?;
                    o.error = Some(anyhow::anyhow!("{msg}{lines}"));
                    Ok(o)
                }
            }
        }
        Command::Show { name, section } => {
            let (_, id) = load(cli, env, out)?;
            let (path, text) = find::show(&id, &name.join(" "), section.as_deref())?;
            out.emit(Envelope::ok(json!({ "path": path, "text": text })), |t| {
                t.push(&text)
            })
        }
        Command::Links {
            name,
            inbound,
            outbound,
            orphans,
        } => {
            let (_, id) = load(cli, env, out)?;
            if *orphans {
                let hits: Vec<_> = find::orphans(&id)
                    .into_iter()
                    .map(|i| find::Hit::of(&id.entries[i]))
                    .collect();
                return out.emit(Envelope::ok(json!({ "orphans": hits })), |t| {
                    for h in &hits {
                        t.line(h.render());
                    }
                });
            }
            let i = find::one(&id, &name.join(" "))?;
            let both = !inbound && !outbound;
            let back = if *inbound || both {
                find::backlinks(&id, i)
            } else {
                vec![]
            };
            let fwd = if *outbound || both {
                find::outlinks(&id, i)
            } else {
                vec![]
            };
            out.emit(
                Envelope::ok(json!({ "path": id.entries[i].rel, "in": back, "out": fwd })),
                |t| t.push(find::render_links(&back, &fwd)),
            )
        }
        Command::Context {
            voice,
            look,
            with,
            notes,
            format,
        } => {
            let (r, id) = load(cli, env, out)?;
            let project = need_project(&id.dir, r.project.as_deref())?;
            let req = context::Request {
                voice: *voice,
                look: *look,
                project,
                with,
                notes: *notes,
            };
            let ctx = context::build(&id, &req)?;
            let envelope =
                Envelope::ok(serde_json::to_value(&ctx)?).with_warnings(ctx.warnings.clone());
            out.emit(envelope, |t| {
                if format == "text" {
                    t.push(context::render_text(&id, &ctx));
                } else {
                    t.push(context::render_paths(&ctx));
                }
            })
        }
        Command::State { days } => {
            let (r, id) = load(cli, env, out)?;
            let mut s = status::compute(&id, *days, 5);
            if let Some(p) = need_project(&id.dir, r.project.as_deref())? {
                s.project = Some(status::project(&id, p));
            }
            out.emit(Envelope::ok(serde_json::to_value(&s)?), |t| {
                t.push(status::render(&s))
            })
        }
        Command::Search {
            query,
            scope,
            types,
            since,
            limit,
        } => {
            let (_, id) = load(cli, env, out)?;
            let hits = search::search(
                &id,
                &query.join(" "),
                &search::Filter {
                    scope: scope.as_deref(),
                    types,
                    project: cli.project.as_deref(),
                    since: since.map(|d| d.to_string()).as_deref(),
                },
                *limit,
            );
            let ok = !hits.is_empty();
            let envelope =
                Envelope::new(ok, json!({ "hits": hits })).with_errors((!ok).then_some("no hits"));
            let mut o = out.emit(envelope, |t| t.push(search::render(&hits)))?;
            o.error = (!ok).then(|| anyhow::anyhow!("no hits"));
            Ok(o)
        }
        Command::Sync {
            action:
                SyncAction::Start {
                    session,
                    transcript,
                    format,
                    now,
                    every,
                },
        } => sync_cmd(cli, env, out, session, *now, *every, |req| {
            sync::start(req, transcript, format)
        }),
        Command::Sync {
            action:
                SyncAction::Push {
                    session,
                    turns,
                    now,
                    every,
                },
        } => {
            // The stdin text is read inside the step, after the config resolves.
            sync_cmd(cli, env, out, session, *now, *every, |req| {
                let turns = match turns {
                    Some(t) => t.clone(),
                    None => std::io::read_to_string(stdin)?,
                };
                sync::push(req, &turns)
            })
        }
        Command::Sync {
            action: SyncAction::Done { session, until },
        } => {
            let r = resolved(cli, env, out)?;
            let doc = sync::done(&r.root, session, *until)?;
            out.emit(Envelope::ok(doc), |t| {
                t.say(format!("session {session} synced until {until}"))
            })
        }
        Command::Migrate {
            from: MigrateFrom::V1 { path, out: dest },
        } => {
            let r = migrate::v1(path, dest)?;
            out.emit(Envelope::ok(serde_json::to_value(&r)?), |t| {
                for (a, b) in &r.moved {
                    t.say(format!("{a} -> {b}"));
                }
                t.push(migrate::render(&r));
            })
        }
        Command::Review { action: None } => {
            let (_, id) = load(cli, env, out)?;
            let p = review::packet(&id);
            out.emit(Envelope::ok(serde_json::to_value(&p)?), |t| {
                t.push(review::render(&p))
            })
        }
        Command::Review {
            action: Some(ReviewAction::Done { through }),
        } => {
            let (_, id) = load(cli, env, out)?;
            let review = review::done(&id, *through)?;
            let through = review["reviewed_through"]
                .as_str()
                .unwrap_or("-")
                .to_string();
            out.emit(Envelope::ok(review), |t| {
                t.say(format!("review recorded through {through}"))
            })
        }
        Command::Soul {
            action:
                SoulAction::Note {
                    file,
                    change,
                    evidence,
                },
        } => {
            let (_, id) = load(cli, env, out)?;
            let note = review::soul_note(&id, file, change, evidence)?;
            let line = format!(
                "noted {}: {}",
                note.record["file"].as_str().unwrap_or(""),
                note.record["change"].as_str().unwrap_or("")
            );
            out.emit(Envelope::ok(note.record).with_warnings(note.warning), |t| {
                t.say(line)
            })
        }
        Command::Soul {
            action: SoulAction::History { file },
        } => {
            let (_, id) = load(cli, env, out)?;
            let list = review::soul_history(&id, file.as_deref());
            out.emit(Envelope::ok(json!({ "history": list })), |t| {
                t.push(review::render_history(&list));
                if list.is_empty() {
                    t.say("no soul changes recorded");
                }
            })
        }
        Command::Changes { since, days } => {
            let (_, id) = load(cli, env, out)?;
            let since = since.clone().or_else(|| {
                // The local day, which Git reads a plain --since day in.
                days.map(|d| (chrono::Local::now() - chrono::Duration::days(d)).format("%Y-%m-%d"))
                    .map(|d| d.to_string())
            });
            let c = changes::changes(&id, since.as_deref())?;
            out.emit(Envelope::ok(serde_json::to_value(&c)?), |t| {
                t.push(changes::render(&c))
            })
        }
        Command::Export {
            format:
                ExportFormat::Card {
                    spec,
                    out: file,
                    png,
                    scenario,
                },
        } => {
            let (_, id) = load(cli, env, out)?;
            let project = need_project(&id.dir, cli.project.as_deref())?;
            let scenario = scenario.as_deref();
            let card = export::card(&id, spec, project, scenario)?;
            let warnings = export::warnings(&card);
            let Some(f) = file else {
                let text = serde_json::to_string_pretty(&card)?;
                return out.emit(Envelope::ok(card).with_warnings(warnings), |t| {
                    t.line(&text)
                });
            };
            export::write_card(&id, &card, project, scenario, f, png.as_deref())?;
            out.emit(
                Envelope::ok(json!({ "path": f })).with_warnings(warnings),
                |t| t.say(format!("wrote {}", f.display())),
            )
        }
        Command::Export {
            format: ExportFormat::Skill { out: dir },
        } => {
            let (_, id) = load(cli, env, out)?;
            let path = export::skill(&id, dir)?;
            out.emit(Envelope::ok(json!({ "path": path })), |t| {
                t.say(format!("wrote {path}"))
            })
        }
        Command::Export {
            format: ExportFormat::Soulspec { out: dir },
        } => {
            let (_, id) = load(cli, env, out)?;
            let report = export::soulspec(&id, dir)?;
            let envelope = Envelope::ok(json!({ "dir": dir, "files": report.files }))
                .with_warnings(report.warnings);
            out.emit(envelope, |t| {
                for f in &report.files {
                    t.say(format!("wrote {f}"));
                }
            })
        }
    }
}

fn templates_cmd(
    cli: &Cli,
    action: Option<&TemplatesAction>,
    env: &Env,
    out: &mut Out,
) -> Result<Output> {
    let root = resolved(cli, env, out)?.root;
    let all = templates::all();
    match action {
        None | Some(TemplatesAction::List) => {
            let rows: Vec<_> = all
                .iter()
                .map(|(name, file, _)| (*name, templates::override_path(&root, file)))
                .collect();
            let list: Vec<_> = rows
                .iter()
                .map(|(name, path)| json!({ "name": name, "override": path }))
                .collect();
            out.emit(Envelope::ok(json!({ "templates": list })), |t| {
                for (name, path) in &rows {
                    match path {
                        Some(p) => t.line(format!("{name:<28} override {}", p.display())),
                        None => t.line(format!("{name:<28} built-in")),
                    }
                }
            })
        }
        Some(TemplatesAction::Show { name }) => {
            let Some((name, file, builtin)) = all.iter().find(|(n, _, _)| n == name) else {
                anyhow::bail!("unknown template {name:?}; run `owo templates` for the list");
            };
            let text = templates::get(&root, file, builtin);
            let path = templates::override_path(&root, file);
            out.emit(
                Envelope::ok(json!({ "name": name, "override": path, "text": text })),
                |t| t.push(&text),
            )
        }
    }
}
