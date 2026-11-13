//! The CLI: maps each parsed command to its library call, and renders the typed result with
//! `render`.

use crate::cli::{
    Cli, Command, ExportFormat, MigrateFrom, ReviewAction, SoulAction, SyncAction, TemplatesAction,
};
use crate::output::{Out, Output};
use crate::render::{self, load, open};
use anyhow::Result;
use owo::{Env, Select};
use std::io::Read;

/// Runs one command and returns what it produced. `stdin` gives the text for `owo write` and
/// `owo sync push` when they take no `--content` or `--turns`; `run` reads it only when the
/// command needs the text. `Command::Mcp` is a server and `Command::Acp` a proxy, not
/// commands, so they return an error here.
pub fn run(cli: &Cli, env: &Env, stdin: &mut dyn Read) -> Output {
    let out = Out {
        json: cli.json,
        quiet: cli.quiet,
        warnings: vec![],
    };
    render::finish(out, |out| dispatch(cli, &select(cli), env, stdin, out))
}

/// The root, identity and project the global flags pick.
pub fn select(cli: &Cli) -> Select {
    Select {
        root: cli.root.clone(),
        identity: cli.identity.clone(),
        project: cli.project.clone(),
    }
}

fn dispatch(
    cli: &Cli,
    sel: &Select,
    env: &Env,
    stdin: &mut dyn Read,
    out: &mut Out,
) -> Result<Output> {
    match &cli.command {
        Command::Slug { text } => render::slug(out, &owo::slug(&text.join(" "))?),
        Command::Templates { action } => {
            let root = open(sel, env, out)?;
            match action {
                None | Some(TemplatesAction::List) => render::templates(out, &root.templates()),
                Some(TemplatesAction::Show { name }) => {
                    let (tpl, text) = root.template(name)?;
                    render::template(out, &tpl, &text)
                }
            }
        }
        Command::Mcp { .. } => anyhow::bail!("`owo mcp` is a server; serve it with `mcp::serve`"),
        Command::Acp { .. } => anyhow::bail!("`owo acp` is a proxy; run it with `owo::acp`"),
        Command::Config => {
            let r = open(sel, env, out)?.config;
            render::config(out, &r)
        }
        Command::New { name, slug } => {
            let c = open(sel, env, out)?.create(name, slug.as_deref())?;
            render::created(out, &c)
        }
        Command::List => {
            let root = open(sel, env, out)?;
            render::list(out, root.path(), &root.list())
        }
        Command::Lint { rules, changed } => {
            let id = load(sel, env, out)?;
            render::lint(out, &id.slug, &id.lint(rules, *changed))
        }
        Command::Write { path, content } => {
            let id = load(sel, env, out)?;
            let text = match content {
                Some(c) => c.clone(),
                None => std::io::read_to_string(stdin)?,
            };
            render::write(out, path, id.write(path, &text)?)
        }
        Command::Add {
            kind,
            title,
            date,
            parent,
            aliases,
            kb,
        } => {
            let id = load(sel, env, out)?;
            let (path, created) = id.add(&owo::AddRequest {
                kind: *kind,
                title: &title.join(" "),
                date: *date,
                parent: parent.as_deref(),
                aliases,
                kb: kb.as_deref(),
            })?;
            render::add(out, *kind, &path, created)
        }
        Command::Stamp { check } => {
            let id = load(sel, env, out)?;
            render::stamp(out, *check, &id.stamp(*check)?)
        }
        Command::Index { check } => {
            let id = load(sel, env, out)?;
            let paths = if *check {
                id.stale_index()
            } else {
                id.index()?
            };
            render::index(out, *check, &paths)
        }
        Command::Resolve { name, kind } => {
            let id = load(sel, env, out)?;
            let name = name.join(" ");
            render::resolve(out, &name, id.resolve(&name, *kind))
        }
        Command::Show { name, section } => {
            let id = load(sel, env, out)?;
            let (path, text) = id.show(&name.join(" "), section.as_deref())?;
            render::show(out, &path, &text)
        }
        Command::Links {
            name,
            inbound,
            outbound,
            orphans,
        } => {
            let id = load(sel, env, out)?;
            if *orphans {
                return render::orphans(out, &id.orphans());
            }
            render::links(out, &id.links(&name.join(" "), *inbound, *outbound)?)
        }
        Command::Context {
            voice,
            look,
            with,
            notes,
            format,
        } => {
            let id = load(sel, env, out)?;
            let ctx = id.context(&owo::ContextRequest {
                voice: *voice,
                look: *look,
                with,
                notes: *notes,
            })?;
            render::context(out, &id, &ctx, format == "paths")
        }
        Command::State { days } => {
            let id = load(sel, env, out)?;
            render::state(out, &id.state(&owo::StateRequest { days: *days })?)
        }
        Command::Search {
            query,
            scope,
            types,
            since,
            limit,
        } => {
            let id = load(sel, env, out)?;
            let req = owo::SearchRequest {
                scope: *scope,
                types,
                // The project filter comes from the flag only.
                project: cli.project.as_deref(),
                since: *since,
                limit: *limit,
            };
            render::search(out, &id.search(&query.join(" "), &req))
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
        } => {
            let s = open(sel, env, out)?.sync()?.start(
                session,
                transcript,
                *format,
                &owo::SyncOptions {
                    now: *now,
                    every: *every,
                },
            )?;
            render::synced(out, &s)
        }
        Command::Sync {
            action:
                SyncAction::Push {
                    session,
                    turns,
                    now,
                    every,
                },
        } => {
            let root = open(sel, env, out)?;
            let sync = root.sync()?;
            // The stdin text is read after the config resolves.
            let turns = match turns {
                Some(t) => t.clone(),
                None => std::io::read_to_string(stdin)?,
            };
            // The turns are parsed before the call, so a bad turns text fails before the session
            // checks.
            let turns = owo::parse_turns(&turns)?;
            render::synced(
                out,
                &sync.push(
                    session,
                    &turns,
                    &owo::SyncOptions {
                        now: *now,
                        every: *every,
                    },
                )?,
            )
        }
        Command::Sync {
            action: SyncAction::Done { session, until },
        } => {
            let doc = open(sel, env, out)?.sync_done(session, *until)?;
            render::sync_done(out, session, *until, &doc)
        }
        Command::Migrate {
            from: MigrateFrom::V1 { path, out: dest },
        } => render::migrate(out, &owo::migrate_v1(path, dest)?),
        Command::Review { action: None } => {
            let id = load(sel, env, out)?;
            render::review(out, &id.review())
        }
        Command::Review {
            action: Some(ReviewAction::Done { through }),
        } => {
            let id = load(sel, env, out)?;
            render::review_done(out, &id.review_done(*through)?)
        }
        Command::Soul {
            action:
                SoulAction::Note {
                    file,
                    change,
                    evidence,
                },
        } => {
            let id = load(sel, env, out)?;
            render::soul_note(out, id.soul_note(file, change, evidence)?)
        }
        Command::Soul {
            action: SoulAction::History { file },
        } => {
            let id = load(sel, env, out)?;
            render::soul_history(out, &id.soul_history(file.as_deref()))
        }
        Command::Changes { since, days } => {
            let id = load(sel, env, out)?;
            let req = owo::ChangesRequest {
                since: since.as_deref(),
                days: *days,
            };
            render::changes(out, &id.changes(&req)?)
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
            let id = load(sel, env, out)?;
            let card = id.export_card(&owo::CardRequest {
                spec: *spec,
                // The project filter comes from the flag only.
                project: cli.project.as_deref(),
                scenario: scenario.as_deref(),
                out: file.as_deref(),
                png: png.as_deref(),
            })?;
            render::card(out, card, file.as_deref())
        }
        Command::Export {
            format: ExportFormat::Skill { out: dir },
        } => {
            let id = load(sel, env, out)?;
            render::skill(out, &id.export_skill(dir)?)
        }
        Command::Export {
            format: ExportFormat::Soulspec { out: dir },
        } => {
            let id = load(sel, env, out)?;
            render::soulspec(out, dir, id.export_soulspec(dir)?)
        }
    }
}
