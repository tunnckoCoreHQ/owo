//! owo: one world, one identity. CLI for living agent identities.

mod add;
mod config;
mod context;
mod create;
mod find;
mod identity;
mod index;
mod lint;
mod markdown;
mod output;
mod review;
mod search;
mod slug;
mod stamp;
mod state;
mod status;
mod templates;

use anyhow::Result;
use clap::{Parser, Subcommand};
use output::{exit, Envelope, Level};
use serde_json::json;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "owo",
    version,
    about = "One world, one identity: manage living agent identities"
)]
struct Cli {
    /// Identities root (else OWO_ROOT, .owo.toml, user config, XDG default)
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    /// Identity slug (else OWO_IDENTITY, .owo.toml, user config, the only identity)
    #[arg(short = 'i', long, global = true)]
    identity: Option<String>,
    /// Project slug (else OWO_PROJECT, .owo.toml)
    #[arg(long, global = true)]
    project: Option<String>,
    /// Print the JSON envelope instead of text
    #[arg(long, global = true)]
    json: bool,
    /// Print only problems
    #[arg(short, long, global = true)]
    quiet: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new identity from the built-in templates
    New {
        /// Display name, for example "Thea"
        name: String,
        /// Slug override (default: slug of the name)
        #[arg(long)]
        slug: Option<String>,
    },
    /// List identities in the root
    List,
    /// Create an entry from its template at its deterministic path
    Add {
        /// Entry type: character, location, location-detail, culture, faction, event, era, thread, conflict, lesson, writing, log, journal, project, note
        #[arg(value_name = "TYPE")]
        kind: String,
        /// Title (for log: optional day title)
        title: Vec<String>,
        /// Date for dated entries (default: today)
        #[arg(long)]
        date: Option<String>,
        /// Parent location for location-detail
        #[arg(long)]
        parent: Option<String>,
        /// Alias (repeatable)
        #[arg(long = "alias")]
        aliases: Vec<String>,
    },
    /// Refresh state.json files from content hashes
    Stamp {
        /// Report drift and exit 1 instead of writing
        #[arg(long)]
        check: bool,
    },
    /// Validate an identity
    Lint {
        /// Run only these rules (repeatable)
        #[arg(long = "rule", value_parser = clap::builder::PossibleValuesParser::new(lint::RULES))]
        rules: Vec<String>,
    },
    /// Regenerate _index.md files and life/timeline.md
    Index {
        /// Report stale generated files and exit 1 instead of writing
        #[arg(long)]
        check: bool,
    },
    /// Print the slug for a text
    Slug { text: Vec<String> },
    /// List or show built-in templates
    Templates {
        /// Template name to print (omit to list)
        name: Option<String>,
    },
    /// Resolve a name to an entry path the way links do
    Resolve {
        name: Vec<String>,
        /// Limit to one entry type
        #[arg(long = "type")]
        kind: Option<String>,
    },
    /// Print an entry, or one section of it
    Show {
        name: Vec<String>,
        #[arg(long)]
        section: Option<String>,
    },
    /// Backlinks and outlinks of an entry, or every orphan
    Links {
        name: Vec<String>,
        /// Only links pointing at the entry
        #[arg(long = "in")]
        inbound: bool,
        /// Only links going out of the entry
        #[arg(long = "out")]
        outbound: bool,
        /// List entries nothing links to
        #[arg(long)]
        orphans: bool,
    },
    /// The tiered load set: core files, the generated Now block, and requested tiers
    Context {
        /// Add soul/VOICE.md
        #[arg(long)]
        voice: bool,
        /// Add soul/LOOK.md
        #[arg(long)]
        look: bool,
        /// Add a resolvable entry (repeatable)
        #[arg(long = "with")]
        with: Vec<String>,
        /// Recent project notes to include
        #[arg(long, default_value_t = 3)]
        notes: usize,
        /// paths: the list; text: file contents concatenated
        #[arg(long, default_value = "paths", value_parser = ["paths", "text"])]
        format: String,
        /// Leave work/state.json untouched
        #[arg(long)]
        no_touch: bool,
    },
    /// Derived current state: activity, recent days and events, open threads, review
    State,
    /// Files updated in the last N days
    Recent {
        #[arg(long, default_value_t = 7)]
        days: i64,
    },
    /// BM25 search over titles, aliases, summaries and bodies
    Search {
        query: Vec<String>,
        /// Limit to soul, life or work
        #[arg(long)]
        scope: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Review bookkeeping
    Review {
        #[command(subcommand)]
        action: ReviewAction,
    },
    /// Show how root, identity and project resolve
    Config,
}

struct Out {
    json: bool,
    quiet: bool,
}

impl Out {
    fn say(&self, text: impl AsRef<str>) {
        if !self.json && !self.quiet {
            println!("{}", text.as_ref());
        }
    }
}

fn main() -> ExitCode {
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

#[derive(Debug)]
struct ConfigError(String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

fn resolved(cli: &Cli) -> Result<config::Resolved> {
    let inputs =
        config::Inputs::from_env(cli.root.clone(), cli.identity.clone(), cli.project.clone());
    config::resolve(&inputs).map_err(|e| ConfigError(format!("{e:#}")).into())
}

fn need_identity(r: &config::Resolved) -> Result<String> {
    r.identity.clone().ok_or_else(|| {
        ConfigError(format!(
            "no identity selected in {}: pass -i <slug> or set OWO_IDENTITY",
            r.root.display()
        ))
        .into()
    })
}

fn run(cli: &Cli, out: &Out) -> Result<i32> {
    match &cli.command {
        Command::Slug { text } => {
            let value = slug::slugify(&text.join(" "));
            if out.json {
                Envelope::ok(json!({ "slug": value })).print();
            } else {
                println!("{value}");
            }
            Ok(if value.is_empty() {
                exit::PROBLEMS
            } else {
                exit::OK
            })
        }
        Command::Templates { name } => templates_cmd(name.as_deref(), out),
        Command::Config => {
            let r = resolved(cli)?;
            if out.json {
                Envelope::ok(serde_json::to_value(&r)?).print();
            } else {
                println!("root: {} ({:?})", r.root.display(), r.root_source);
                println!("identity: {}", r.identity.as_deref().unwrap_or("-"));
                println!("project: {}", r.project.as_deref().unwrap_or("-"));
            }
            Ok(exit::OK)
        }
        Command::New {
            name,
            slug: slug_flag,
        } => {
            let r = resolved(cli)?;
            let slug_value = slug_flag.clone().unwrap_or_else(|| slug::slugify(name));
            let created = create::new_identity(&r.root, name.trim(), &slug_value)?;
            if out.json {
                Envelope::ok(
                    json!({ "slug": created.slug, "path": created.dir, "files": created.files }),
                )
                .print();
            } else {
                out.say(format!(
                    "created {} ({} files)",
                    created.dir.display(),
                    created.files.len()
                ));
                for f in &created.files {
                    out.say(format!("  {f}"));
                }
            }
            Ok(exit::OK)
        }
        Command::List => {
            let r = resolved(cli)?;
            let slugs = config::list_identities(&r.root);
            if out.json {
                Envelope::ok(json!({ "root": r.root, "identities": slugs })).print();
            } else {
                for s in &slugs {
                    println!("{s}");
                }
            }
            Ok(exit::OK)
        }
        Command::Lint { rules } => {
            let r = resolved(cli)?;
            let slug = need_identity(&r)?;
            let id = identity::Identity::load(&r.root, &slug)?;
            let found = lint::run(
                &id,
                &lint::Options {
                    rules: rules.clone(),
                },
            );
            let errors = found.iter().filter(|d| d.level == Level::Error).count();
            let warnings = found.iter().filter(|d| d.level == Level::Warning).count();
            if out.json {
                let split = |level: Level| {
                    found
                        .iter()
                        .filter(|d| d.level == level)
                        .map(|d| serde_json::to_value(d).expect("serializes"))
                        .collect::<Vec<_>>()
                };
                let env = Envelope {
                    ok: errors == 0,
                    data: json!({ "identity": slug, "infos": split(Level::Info) }),
                    warnings: split(Level::Warning),
                    errors: split(Level::Error),
                };
                env.print();
            } else {
                for d in found
                    .iter()
                    .filter(|d| !out.quiet || d.level == Level::Error)
                {
                    println!("{}", d.render());
                }
                out.say(format!("{slug}: {errors} errors, {warnings} warnings"));
            }
            Ok(if errors > 0 { exit::PROBLEMS } else { exit::OK })
        }
        Command::Add {
            kind,
            title,
            date,
            parent,
            aliases,
        } => {
            let r = resolved(cli)?;
            let slug = need_identity(&r)?;
            let id = identity::Identity::load(&r.root, &slug)?;
            let title = if title.is_empty() && kind == "log" {
                "Day".to_string()
            } else {
                title.join(" ")
            };
            let req = add::Request {
                kind,
                title: &title,
                date: date.as_deref(),
                parent: parent.as_deref(),
                project: r.project.as_deref(),
                aliases,
            };
            let (path, created) = match add::add(&id, &req)? {
                add::Added::Created(p) => (p, true),
                add::Added::Exists(p) => (p, false),
            };
            let ok = created || kind == "log";
            if out.json {
                let env = Envelope {
                    ok,
                    data: json!({ "path": path, "created": created }),
                    warnings: vec![],
                    errors: vec![],
                };
                env.print();
            } else if created {
                println!("{path}");
            } else {
                println!("exists {path}");
            }
            Ok(if ok { exit::OK } else { exit::PROBLEMS })
        }
        Command::Stamp { check } => {
            let r = resolved(cli)?;
            let slug = need_identity(&r)?;
            let id = identity::Identity::load(&r.root, &slug)?;
            let report = stamp::run(&id, *check)?;
            let clean = report.is_clean();
            if out.json {
                let env = Envelope {
                    ok: !*check || clean,
                    data: serde_json::to_value(&report)?,
                    warnings: vec![],
                    errors: vec![],
                };
                env.print();
            } else {
                for (label, list) in [
                    ("added", &report.added),
                    ("changed", &report.changed),
                    ("removed", &report.removed),
                ] {
                    for p in list {
                        println!("{label} {p}");
                    }
                }
                if clean {
                    out.say("state is up to date");
                }
            }
            Ok(if *check && !clean {
                exit::PROBLEMS
            } else {
                exit::OK
            })
        }
        Command::Index { check } => {
            let r = resolved(cli)?;
            let slug = need_identity(&r)?;
            let id = identity::Identity::load(&r.root, &slug)?;
            let paths = if *check {
                index::stale(&id)
            } else {
                index::write(&id)?
            };
            if out.json {
                let key = if *check { "stale" } else { "written" };
                let env = Envelope {
                    ok: !*check || paths.is_empty(),
                    data: json!({ key: paths }),
                    warnings: vec![],
                    errors: vec![],
                };
                env.print();
            } else {
                for p in &paths {
                    println!("{}{p}", if *check { "stale " } else { "wrote " });
                }
                if paths.is_empty() {
                    out.say("generated files are up to date");
                }
            }
            Ok(if *check && !paths.is_empty() {
                exit::PROBLEMS
            } else {
                exit::OK
            })
        }
        Command::Resolve { name, kind } => {
            let id = load(cli)?;
            let name = name.join(" ");
            match find::resolve(&id, &name, kind.as_deref()) {
                find::Resolution::Found(i) => {
                    let hit = find::Hit::of(&id.entries[i]);
                    if out.json {
                        Envelope::ok(serde_json::to_value(&hit)?).print();
                    } else {
                        println!("{}", hit.path);
                    }
                    Ok(exit::OK)
                }
                find::Resolution::Ambiguous(v) | find::Resolution::Missing(v) => {
                    let hits: Vec<_> = v.iter().map(|i| find::Hit::of(&id.entries[*i])).collect();
                    let msg = if hits.is_empty() {
                        format!("{name:?} does not resolve")
                    } else {
                        format!("{name:?} does not resolve to one entry")
                    };
                    if out.json {
                        Envelope {
                            ok: false,
                            data: json!({ "candidates": hits }),
                            warnings: vec![],
                            errors: vec![json!(msg)],
                        }
                        .print();
                    } else {
                        eprintln!("owo: {msg}");
                        for h in &hits {
                            eprintln!("  {}", h.render());
                        }
                    }
                    Ok(exit::PROBLEMS)
                }
            }
        }
        Command::Show { name, section } => {
            let id = load(cli)?;
            let i = resolve_one(&id, &name.join(" "))?;
            let text = find::show(&id.entries[i], section.as_deref()).ok_or_else(|| {
                anyhow::anyhow!(
                    "no section {:?} in {}",
                    section.as_deref().unwrap_or(""),
                    id.entries[i].rel
                )
            })?;
            if out.json {
                Envelope::ok(json!({ "path": id.entries[i].rel, "text": text })).print();
            } else {
                print!("{text}");
            }
            Ok(exit::OK)
        }
        Command::Links {
            name,
            inbound,
            outbound,
            orphans,
        } => {
            let id = load(cli)?;
            if *orphans {
                let hits: Vec<_> = find::orphans(&id)
                    .into_iter()
                    .map(|i| find::Hit::of(&id.entries[i]))
                    .collect();
                if out.json {
                    Envelope::ok(json!({ "orphans": hits })).print();
                } else {
                    for h in &hits {
                        println!("{}", h.render());
                    }
                }
                return Ok(exit::OK);
            }
            let i = resolve_one(&id, &name.join(" "))?;
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
            if out.json {
                Envelope::ok(json!({ "path": id.entries[i].rel, "in": back, "out": fwd })).print();
            } else {
                for l in &back {
                    println!("in  {}:{}  {}", l.path, l.line, l.text);
                }
                for l in &fwd {
                    let to = if l.path.is_empty() {
                        "(broken)".to_string()
                    } else {
                        l.path.clone()
                    };
                    println!("out [[{}]] -> {to}  (line {})", l.target, l.line);
                }
            }
            Ok(exit::OK)
        }
        Command::Context {
            voice,
            look,
            with,
            notes,
            format,
            no_touch,
        } => {
            let r = resolved(cli)?;
            let id = identity::Identity::load(&r.root, &need_identity(&r)?)?;
            let req = context::Request {
                voice: *voice,
                look: *look,
                project: r.project.as_deref(),
                config_kb: r.kb.as_ref().map(PathBuf::from),
                with,
                notes: *notes,
            };
            let ctx = context::build(&id, &req)?;
            if let (Some(p), false) = (r.project.as_deref(), *no_touch) {
                context::touch_project(&id, p)?;
            }
            if out.json {
                let warnings = ctx.warnings.iter().map(|w| json!(w)).collect();
                Envelope {
                    ok: true,
                    data: serde_json::to_value(&ctx)?,
                    warnings,
                    errors: vec![],
                }
                .print();
            } else {
                for w in &ctx.warnings {
                    eprintln!("warning: {w}");
                }
                print!(
                    "{}",
                    if format == "text" {
                        context::render_text(&id, &ctx)
                    } else {
                        context::render_paths(&ctx)
                    }
                );
            }
            Ok(exit::OK)
        }
        Command::State => {
            let id = load(cli)?;
            let s = status::compute(&id, 14, 5);
            if out.json {
                Envelope::ok(serde_json::to_value(&s)?).print();
            } else {
                print!("{}", status::render(&s));
            }
            Ok(exit::OK)
        }
        Command::Recent { days } => {
            let id = load(cli)?;
            let list = status::recent(&id, *days);
            if out.json {
                Envelope::ok(json!({ "days": days, "files": list })).print();
            } else {
                for f in &list {
                    println!("{}  {}", &f.updated[..10.min(f.updated.len())], f.path);
                }
            }
            Ok(exit::OK)
        }
        Command::Search {
            query,
            scope,
            limit,
        } => {
            let id = load(cli)?;
            let hits = search::search(&id, &query.join(" "), scope.as_deref(), *limit);
            if out.json {
                Envelope::ok(json!({ "hits": hits })).print();
            } else {
                for h in &hits {
                    println!("{:>6.2}  {}  {}", h.score, h.path, h.title);
                    if let (Some(l), Some(s)) = (h.line, &h.snippet) {
                        println!("        {l}: {s}");
                    }
                }
            }
            Ok(if hits.is_empty() {
                exit::PROBLEMS
            } else {
                exit::OK
            })
        }
        Command::Review {
            action: ReviewAction::Done,
        } => {
            let id = load(cli)?;
            let review = review::done(&id)?;
            if out.json {
                Envelope::ok(review).print();
            } else {
                out.say(format!(
                    "review recorded through {}",
                    review["reviewed_through"].as_str().unwrap_or("-")
                ));
            }
            Ok(exit::OK)
        }
    }
}

#[derive(Subcommand)]
enum ReviewAction {
    /// Record a finished review (dates and the MEMORY.md hash)
    Done,
}

fn load(cli: &Cli) -> Result<identity::Identity> {
    let r = resolved(cli)?;
    identity::Identity::load(&r.root, &need_identity(&r)?)
}

fn resolve_one(id: &identity::Identity, name: &str) -> Result<usize> {
    match find::resolve(id, name, None) {
        find::Resolution::Found(i) => Ok(i),
        find::Resolution::Ambiguous(v) | find::Resolution::Missing(v) => {
            let c: Vec<String> = v.iter().map(|i| id.entries[*i].rel.clone()).collect();
            anyhow::bail!(
                "{name:?} does not resolve to one entry{}",
                if c.is_empty() {
                    String::new()
                } else {
                    format!("; candidates: {}", c.join(", "))
                }
            )
        }
    }
}

fn templates_cmd(name: Option<&str>, out: &Out) -> Result<i32> {
    let all: Vec<(&str, &str)> = templates::IDENTITY_FILES
        .iter()
        .chain(templates::ENTRY_TEMPLATES.iter())
        .copied()
        .collect();
    match name {
        None => {
            let names: Vec<&str> = all.iter().map(|(n, _)| *n).collect();
            if out.json {
                Envelope::ok(json!({ "templates": names })).print();
            } else {
                for n in names {
                    println!("{n}");
                }
            }
            Ok(exit::OK)
        }
        Some(n) => match all.iter().find(|(k, _)| *k == n) {
            Some((_, text)) => {
                if out.json {
                    Envelope::ok(json!({ "name": n, "text": text })).print();
                } else {
                    print!("{text}");
                }
                Ok(exit::OK)
            }
            None => {
                if out.json {
                    Envelope::fail(&format!("unknown template {n:?}")).print();
                } else {
                    eprintln!("owo: unknown template {n:?}; run `owo templates` for the list");
                }
                Ok(exit::PROBLEMS)
            }
        },
    }
}
