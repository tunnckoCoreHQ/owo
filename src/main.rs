//! owo: one world, one identity. CLI for living agent identities.

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
mod mcp;
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

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::{
    Cli, Command, ExportFormat, MigrateFrom, ReviewAction, SoulAction, SyncAction, TemplatesAction,
};
use config::{need_identity, need_project, ConfigError};
use identity::Kind;
use output::{exit, Envelope, Level, Out};
use serde_json::json;
use std::process::ExitCode;

fn main() -> ExitCode {
    // Exit quietly when stdout closes early (`owo search x | head`).
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let out = Out {
        json: cli.json,
        quiet: cli.quiet,
    };
    let code = match run(&cli, &out) {
        Ok(code) => code,
        Err(e) => {
            if out.json {
                Envelope::fail(&format!("{e:#}")).print();
            } else {
                eprintln!("owo: {e:#}");
            }
            if e.downcast_ref::<ConfigError>().is_some() {
                exit::CONFIG
            } else if e.downcast_ref::<std::io::Error>().is_some() {
                exit::IO
            } else {
                exit::PROBLEMS
            }
        }
    };
    ExitCode::from(code as u8)
}

fn resolved(cli: &Cli) -> Result<config::Resolved> {
    let inputs =
        config::Inputs::from_env(cli.root.clone(), cli.identity.clone(), cli.project.clone());
    let r = config::resolve(&inputs).map_err(|e| ConfigError(format!("{e:#}")))?;
    if let config::Source::ProjectFile(file) = &r.root_source {
        if file.parent() != Some(inputs.cwd.as_path()) {
            output::warn(&format!(
                "the root comes from {} in a parent folder",
                file.display()
            ));
        }
    }
    Ok(r)
}

fn load(cli: &Cli) -> Result<(config::Resolved, identity::Identity)> {
    let r = resolved(cli)?;
    let id = identity::Identity::load(&r.root, &need_identity(&r)?)?;
    Ok((r, id))
}

/// Runs `sync start` or `sync push` with the shared identity, project and run size.
fn sync_cmd(
    cli: &Cli,
    out: &Out,
    session: &str,
    now: bool,
    every: Option<usize>,
    step: impl FnOnce(&sync::StartRequest) -> Result<sync::Start>,
) -> Result<i32> {
    let r = resolved(cli)?;
    let slug = need_identity(&r)?;
    let dir = r.root.join(&slug);
    let project = need_project(&dir, r.project.as_deref())?;
    let every = every
        .or_else(|| {
            identity::read_json(&dir.join("identity.json"))?
                .get("sync_every")?
                .as_u64()
                .map(|v| v as usize)
        })
        .unwrap_or(6)
        .max(1);
    let s = step(&sync::StartRequest {
        root: &r.root,
        slug: &slug,
        session,
        now,
        every,
        project,
    })?;
    out.emit(Envelope::ok(serde_json::to_value(&s)?), || {
        if let Some(n) = s.added {
            out.say(format!("added {n}"));
        }
        if s.source.is_some() {
            println!("{}", sync::render(&s));
        } else {
            out.say(sync::render(&s));
        }
    })
}

fn run(cli: &Cli, out: &Out) -> Result<i32> {
    match &cli.command {
        Command::Slug { text } => {
            let value = slug::slugify(&text.join(" "));
            let ok = !value.is_empty();
            out.emit(Envelope::new(ok, json!({ "slug": value })), || {
                println!("{value}")
            })
        }
        Command::Templates { action } => templates_cmd(cli, action.as_ref(), out),
        Command::Mcp { remote, port } => {
            match remote {
                Some(addr) => mcp::serve_http(addr, *port, cli)?,
                None => mcp::serve(cli)?,
            }
            Ok(exit::OK)
        }
        Command::Config => {
            let r = resolved(cli)?;
            out.emit(Envelope::ok(serde_json::to_value(&r)?), || {
                println!("root: {} ({:?})", r.root.display(), r.root_source);
                println!("identity: {}", r.identity.as_deref().unwrap_or("-"));
                println!("project: {}", r.project.as_deref().unwrap_or("-"));
            })
        }
        Command::New {
            name,
            slug: slug_flag,
        } => {
            let r = resolved(cli)?;
            let slug_value = slug_flag.clone().unwrap_or_else(|| slug::slugify(name));
            let c = create::new_identity(&r.root, name.trim(), &slug_value)?;
            let data = json!({ "slug": c.slug, "path": c.dir, "files": c.files });
            out.emit(Envelope::ok(data), || {
                out.say(format!(
                    "created {} ({} files)",
                    c.dir.display(),
                    c.files.len()
                ));
                for f in &c.files {
                    out.say(format!("  {f}"));
                }
            })
        }
        Command::List => {
            let r = resolved(cli)?;
            let rows = status::list(&r.root);
            out.emit(
                Envelope::ok(json!({ "root": r.root, "identities": rows })),
                || print!("{}", status::render_list(&rows)),
            )
        }
        Command::Lint { rules, changed } => {
            let (_, id) = load(cli)?;
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
            out.emit(envelope, || {
                for d in found
                    .iter()
                    .filter(|d| !out.quiet || d.level == Level::Error)
                {
                    println!("{}", d.render());
                }
                out.say(format!("{slug}: {errors} errors, {warnings} warnings"));
            })
        }
        Command::Write { path, content } => {
            let (_, id) = load(cli)?;
            let text = match content {
                Some(c) => c.clone(),
                None => std::io::read_to_string(std::io::stdin())?,
            };
            let created = add::write(&id, path, &text)?;
            out.emit(
                Envelope::ok(json!({ "path": path, "created": created })),
                || println!("{path}"),
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
            let (r, id) = load(cli)?;
            let title = title.join(" ");
            if title.trim().is_empty() && *kind != Kind::Log {
                Cli::command()
                    .error(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        format!("`owo add {}` needs a title", find::kind_name(*kind)),
                    )
                    .exit();
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
            out.emit(envelope, || {
                if created {
                    println!("{path}");
                } else {
                    println!("exists {path}");
                }
            })
        }
        Command::Stamp { check } => {
            let (_, id) = load(cli)?;
            let report = stamp::run(&id, *check)?;
            let clean = report.is_clean();
            let envelope = Envelope::new(!*check || clean, serde_json::to_value(&report)?);
            out.emit(envelope, || {
                for (label, list) in [
                    ("added", &report.added),
                    ("changed", &report.changed),
                    ("removed", &report.removed),
                ] {
                    for p in list {
                        out.say(format!("{label} {p}"));
                    }
                }
                if clean {
                    out.say("state is up to date");
                }
            })
        }
        Command::Index { check } => {
            let (_, id) = load(cli)?;
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
            out.emit(envelope, || {
                for p in &paths {
                    out.say(format!("{label}{p}"));
                }
                if paths.is_empty() {
                    out.say("generated files are up to date");
                }
            })
        }
        Command::Resolve { name, kind } => {
            let (_, id) = load(cli)?;
            let name = name.join(" ");
            match find::resolve(&id, &name, kind.as_deref()) {
                find::Resolution::Found(i) => {
                    let hit = find::Hit::of(&id.entries[i]);
                    out.emit(Envelope::ok(serde_json::to_value(&hit)?), || {
                        println!("{}", hit.path)
                    })
                }
                find::Resolution::Ambiguous(v) | find::Resolution::Missing(v) => {
                    let hits: Vec<_> = v.iter().map(|i| find::Hit::of(&id.entries[*i])).collect();
                    let msg = if hits.is_empty() {
                        format!("{name:?} does not resolve")
                    } else {
                        format!("{name:?} does not resolve to one entry")
                    };
                    let envelope = Envelope::new(false, json!({ "candidates": hits }))
                        .with_errors([msg.as_str()]);
                    out.emit(envelope, || {
                        eprintln!("owo: {msg}");
                        for h in &hits {
                            eprintln!("  {}", h.render());
                        }
                    })
                }
            }
        }
        Command::Show { name, section } => {
            let (_, id) = load(cli)?;
            let joined = name.join(" ");
            let target = joined.trim_start_matches("[[").trim_end_matches("]]");
            let (path, text) = match kb::show(&id, target, section.as_deref()) {
                Some(shown) => shown?,
                None => {
                    let e = &id.entries[find::one(&id, &joined)?];
                    let text = find::show(e, section.as_deref()).ok_or_else(|| {
                        anyhow::anyhow!(
                            "no section {:?} in {}",
                            section.as_deref().unwrap_or(""),
                            e.rel
                        )
                    })?;
                    (e.rel.clone(), text)
                }
            };
            out.emit(Envelope::ok(json!({ "path": path, "text": text })), || {
                print!("{text}")
            })
        }
        Command::Links {
            name,
            inbound,
            outbound,
            orphans,
        } => {
            let (_, id) = load(cli)?;
            if *orphans {
                let hits: Vec<_> = find::orphans(&id)
                    .into_iter()
                    .map(|i| find::Hit::of(&id.entries[i]))
                    .collect();
                return out.emit(Envelope::ok(json!({ "orphans": hits })), || {
                    for h in &hits {
                        println!("{}", h.render());
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
                || print!("{}", find::render_links(&back, &fwd)),
            )
        }
        Command::Context {
            voice,
            look,
            with,
            notes,
            format,
        } => {
            let (r, id) = load(cli)?;
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
            out.emit(envelope, || {
                if format == "text" {
                    print!("{}", context::render_text(&id, &ctx));
                } else {
                    print!("{}", context::render_paths(&ctx));
                }
            })
        }
        Command::State { days } => {
            let (r, id) = load(cli)?;
            let mut s = status::compute(&id, *days, 5);
            if let Some(p) = need_project(&id.dir, r.project.as_deref())? {
                s.project = Some(status::project(&id, p));
            }
            out.emit(Envelope::ok(serde_json::to_value(&s)?), || {
                print!("{}", status::render(&s))
            })
        }
        Command::Search {
            query,
            scope,
            types,
            since,
            limit,
        } => {
            let (_, id) = load(cli)?;
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
            out.emit(envelope, || {
                if !ok {
                    eprintln!("owo: no hits");
                }
                print!("{}", search::render(&hits));
            })
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
        } => sync_cmd(cli, out, session, *now, *every, |req| {
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
            let turns = match turns {
                Some(t) => t.clone(),
                None => std::io::read_to_string(std::io::stdin())?,
            };
            sync_cmd(cli, out, session, *now, *every, |req| {
                sync::push(req, &turns)
            })
        }
        Command::Sync {
            action: SyncAction::Done { session, until },
        } => {
            let r = resolved(cli)?;
            let doc = sync::done(&r.root, session, *until)?;
            out.emit(Envelope::ok(doc), || {
                out.say(format!("session {session} synced until {until}"))
            })
        }
        Command::Migrate {
            from: MigrateFrom::V1 { path, out: dest },
        } => {
            let r = migrate::v1(path, dest)?;
            out.emit(Envelope::ok(serde_json::to_value(&r)?), || {
                for (a, b) in &r.moved {
                    out.say(format!("{a} -> {b}"));
                }
                print!("{}", migrate::render(&r));
            })
        }
        Command::Review { action: None } => {
            let (_, id) = load(cli)?;
            let p = review::packet(&id);
            out.emit(Envelope::ok(serde_json::to_value(&p)?), || {
                print!("{}", review::render(&p))
            })
        }
        Command::Review {
            action: Some(ReviewAction::Done { through }),
        } => {
            let (_, id) = load(cli)?;
            let review = review::done(&id, *through)?;
            let through = review["reviewed_through"]
                .as_str()
                .unwrap_or("-")
                .to_string();
            out.emit(Envelope::ok(review), || {
                out.say(format!("review recorded through {through}"))
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
            let (_, id) = load(cli)?;
            let note = review::soul_note(&id, file, change, evidence)?;
            let line = format!(
                "noted {}: {}",
                note.record["file"].as_str().unwrap_or(""),
                note.record["change"].as_str().unwrap_or("")
            );
            out.emit(
                Envelope::ok(note.record).with_warnings(note.warning),
                || out.say(line),
            )
        }
        Command::Soul {
            action: SoulAction::History { file },
        } => {
            let (_, id) = load(cli)?;
            let list = review::soul_history(&id, file.as_deref());
            out.emit(Envelope::ok(json!({ "history": list })), || {
                print!("{}", review::render_history(&list));
                if list.is_empty() {
                    out.say("no soul changes recorded");
                }
            })
        }
        Command::Changes { since, days } => {
            let (_, id) = load(cli)?;
            let since = since.clone().or_else(|| {
                days.map(|d| (chrono::Utc::now() - chrono::Duration::days(d)).format("%Y-%m-%d"))
                    .map(|d| d.to_string())
            });
            let c = changes::changes(&id, since.as_deref())?;
            out.emit(Envelope::ok(serde_json::to_value(&c)?), || {
                print!("{}", changes::render(&c))
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
            let (_, id) = load(cli)?;
            let project = need_project(&id.dir, cli.project.as_deref())?;
            let scenario = scenario.as_deref();
            let card = export::card(&id, spec, project, scenario)?;
            // A card without a greeting or an example message opens on nothing.
            let warnings: Vec<String> = ["first_mes", "mes_example"]
                .iter()
                .filter(|k| {
                    card["data"][**k]
                        .as_str()
                        .is_none_or(|s| s.trim().is_empty())
                })
                .map(|k| format!("{k} is empty; fill soul/examples/"))
                .collect();
            let Some(f) = file else {
                let text = serde_json::to_string_pretty(&card)?;
                return out.emit(Envelope::ok(card).with_warnings(warnings), || {
                    println!("{text}")
                });
            };
            export::write_card(&id, spec, project, scenario, f, png.as_deref())?;
            out.emit(
                Envelope::ok(json!({ "path": f })).with_warnings(warnings),
                || out.say(format!("wrote {}", f.display())),
            )
        }
        Command::Export {
            format: ExportFormat::Skill { out: dir },
        } => {
            let (_, id) = load(cli)?;
            let path = export::skill(&id, dir)?;
            out.emit(Envelope::ok(json!({ "path": path })), || {
                out.say(format!("wrote {path}"))
            })
        }
        Command::Export {
            format: ExportFormat::Soulspec { out: dir },
        } => {
            let (_, id) = load(cli)?;
            let report = export::soulspec(&id, dir)?;
            let envelope = Envelope::ok(json!({ "dir": dir, "files": report.files }))
                .with_warnings(report.warnings);
            out.emit(envelope, || {
                for f in &report.files {
                    out.say(format!("wrote {f}"));
                }
            })
        }
    }
}

fn templates_cmd(cli: &Cli, action: Option<&TemplatesAction>, out: &Out) -> Result<i32> {
    let root = resolved(cli)?.root;
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
            out.emit(Envelope::ok(json!({ "templates": list })), || {
                for (name, path) in &rows {
                    match path {
                        Some(p) => println!("{name:<28} override {}", p.display()),
                        None => println!("{name:<28} built-in"),
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
                || print!("{text}"),
            )
        }
    }
}
