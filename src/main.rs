//! owo: one world, one identity. CLI for living agent identities.

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
        /// KB path for a project, relative to the identity folder
        #[arg(long)]
        kb: Option<String>,
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
        /// Report only files changed since the last commit or stamp
        #[arg(long)]
        changed: bool,
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
    State {
        /// Window for recent events
        #[arg(long, default_value_t = 14)]
        days: i64,
    },
    /// Files updated in the last N days
    Recent {
        #[arg(long, default_value_t = 7)]
        days: i64,
    },
    /// BM25 search over titles, aliases, summaries and bodies
    Search {
        query: Vec<String>,
        /// Limit to soul, life or work
        #[arg(long, visible_alias = "in")]
        scope: Option<String>,
        /// Limit to entry types (repeatable)
        #[arg(long = "type")]
        types: Vec<String>,
        /// Only entries dated (or updated) on or after YYYY-MM-DD
        #[arg(long)]
        since: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Print the review packet, or record a finished review with `done`
    Review {
        #[command(subcommand)]
        action: Option<ReviewAction>,
    },
    /// Soul history: evidence-backed records of soul changes
    Soul {
        #[command(subcommand)]
        action: SoulAction,
    },
    /// What changed, grouped by day (Git history, else state stamps)
    Changes {
        /// YYYY-MM-DD or anything `git log --since` accepts
        #[arg(long)]
        since: Option<String>,
    },
    /// Export the identity to another format
    Export {
        #[command(subcommand)]
        format: ExportFormat,
    },
    /// Session sync: slice a host transcript into sources/ for ingest
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    /// Migrate an identity from an older layout
    Migrate {
        #[command(subcommand)]
        from: MigrateFrom,
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
            let rows = status::list(&r.root);
            if out.json {
                Envelope::ok(json!({ "root": r.root, "identities": rows })).print();
            } else {
                for row in &rows {
                    let last = row
                        .last_activity
                        .as_deref()
                        .map(|t| &t[..10.min(t.len())])
                        .unwrap_or("-");
                    println!(
                        "{:<16} {:<20} {:<10} {}",
                        row.slug,
                        row.name,
                        last,
                        row.projects.join(", ")
                    );
                }
            }
            Ok(exit::OK)
        }
        Command::Lint { rules, changed } => {
            let r = resolved(cli)?;
            let slug = need_identity(&r)?;
            let id = identity::Identity::load(&r.root, &slug)?;
            let mut found = lint::run(
                &id,
                &lint::Options {
                    rules: rules.clone(),
                },
            );
            if *changed {
                let paths = changes::changed_paths(&id);
                found.retain(|d| paths.contains(&d.path));
            }
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
            kb,
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
                kb: kb.as_deref(),
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
                        out.say(format!("{label} {p}"));
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
                    out.say(format!("{}{p}", if *check { "stale " } else { "wrote " }));
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
            let joined = name.join(" ");
            if let Some(res) =
                find::kb_link(&id, joined.trim_start_matches("[[").trim_end_matches("]]"))
            {
                let k = res.map_err(|m| anyhow::anyhow!(m))?;
                let file = k.file.clone().filter(|_| k.file_exists).ok_or_else(|| {
                    anyhow::anyhow!(
                        "KB file not found: {}",
                        k.file.clone().unwrap_or_else(|| "(no KB path)".into())
                    )
                })?;
                let text = std::fs::read_to_string(&file)?;
                let wanted = section.clone().or(k.section.clone());
                let body = match &wanted {
                    None => text,
                    Some(s) => {
                        let mut out = vec![];
                        let mut level = 0;
                        for line in text.lines() {
                            let hashes = line.chars().take_while(|c| *c == '#').count();
                            if level > 0 && hashes > 0 && hashes <= level {
                                break;
                            }
                            if level == 0
                                && hashes > 0
                                && line[hashes..].trim().eq_ignore_ascii_case(s)
                            {
                                level = hashes;
                            }
                            if level > 0 {
                                out.push(line);
                            }
                        }
                        if out.is_empty() {
                            anyhow::bail!("no section {s:?} in {file}");
                        }
                        format!("{}\n", out.join("\n"))
                    }
                };
                if out.json {
                    Envelope::ok(json!({ "path": file, "text": body })).print();
                } else {
                    print!("{body}");
                }
                return Ok(exit::OK);
            }
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
        Command::State { days } => {
            let r = resolved(cli)?;
            let id = identity::Identity::load(&r.root, &need_identity(&r)?)?;
            let mut s = status::compute(&id, *days, 5);
            if let Some(p) = r.project.as_deref() {
                s.project = Some(status::project(&id, p)?);
            }
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
            types,
            since,
            limit,
        } => {
            let id = load(cli)?;
            let hits = search::search(
                &id,
                &query.join(" "),
                &search::Filter {
                    scope: scope.as_deref(),
                    types,
                    project: cli.project.as_deref(),
                    since: since.as_deref(),
                },
                *limit,
            );
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
            let r = resolved(cli)?;
            let slug = need_identity(&r)?;
            let id = identity::Identity::load(&r.root, &slug)?;
            let every = every
                .or_else(|| {
                    id.manifest
                        .as_ref()
                        .and_then(|m| m.get("sync_every"))
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize)
                })
                .unwrap_or(6)
                .max(1);
            let req = sync::StartRequest {
                root: &r.root,
                slug: &slug,
                session,
                transcript,
                format,
                now: *now,
                every,
                project: r.project.as_deref(),
            };
            let s = sync::start(&req)?;
            if out.json {
                Envelope::ok(serde_json::to_value(&s)?).print();
            } else if let Some(src) = &s.source {
                println!(
                    "due {} {}-{}\nsource {src}\nprompt {}",
                    s.identity,
                    s.from,
                    s.until,
                    s.prompt.as_deref().unwrap_or("")
                );
            } else {
                out.say(format!(
                    "not due: {} new user turns (every {})",
                    s.new_user_turns, s.every
                ));
            }
            Ok(exit::OK)
        }
        Command::Sync {
            action: SyncAction::Done { session, until },
        } => {
            let r = resolved(cli)?;
            let doc = sync::done(&r.root, session, *until)?;
            if out.json {
                Envelope::ok(doc).print();
            } else {
                out.say(format!("session {session} synced until {until}"));
            }
            Ok(exit::OK)
        }
        Command::Migrate {
            from: MigrateFrom::V1 { path, out: dest },
        } => {
            let r = migrate::v1(path, dest)?;
            if out.json {
                Envelope::ok(serde_json::to_value(&r)?).print();
            } else {
                for (a, b) in &r.moved {
                    out.say(format!("{a} -> {b}"));
                }
                for s in &r.skipped {
                    println!("skipped {s}");
                }
                println!("\nChecklist for {}:", r.dir);
                for c in &r.checklist {
                    println!("- [ ] {c}");
                }
            }
            Ok(exit::OK)
        }
        Command::Review { action: None } => {
            let id = load(cli)?;
            let p = review::packet(&id);
            if out.json {
                Envelope::ok(serde_json::to_value(&p)?).print();
            } else {
                print!("{}", review::render(&p));
            }
            Ok(exit::OK)
        }
        Command::Soul {
            action:
                SoulAction::Note {
                    file,
                    change,
                    evidence,
                },
        } => {
            let id = load(cli)?;
            let note = review::soul_note(&id, file, change, evidence)?;
            if out.json {
                let warnings = note.warning.iter().map(|w| json!(w)).collect();
                Envelope {
                    ok: true,
                    data: note.record,
                    warnings,
                    errors: vec![],
                }
                .print();
            } else {
                if let Some(w) = &note.warning {
                    eprintln!("warning: {w}");
                }
                out.say(format!(
                    "noted {}: {}",
                    note.record["file"].as_str().unwrap_or(""),
                    note.record["change"].as_str().unwrap_or("")
                ));
            }
            Ok(exit::OK)
        }
        Command::Soul {
            action: SoulAction::History { file },
        } => {
            let id = load(cli)?;
            let list = review::soul_history(&id, file.as_deref());
            if out.json {
                Envelope::ok(json!({ "history": list })).print();
            } else {
                for r in &list {
                    let ev: Vec<&str> = r["evidence"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();
                    println!(
                        "{}  {}  {}\n    evidence: {}",
                        r["at"].as_str().unwrap_or("?"),
                        r["file"].as_str().unwrap_or("?"),
                        r["change"].as_str().unwrap_or(""),
                        ev.join(", ")
                    );
                }
                if list.is_empty() {
                    out.say("no soul changes recorded");
                }
            }
            Ok(exit::OK)
        }
        Command::Changes { since } => {
            let id = load(cli)?;
            let c = changes::changes(&id, since.as_deref());
            if out.json {
                Envelope::ok(serde_json::to_value(&c)?).print();
            } else {
                print!("{}", changes::render(&c));
            }
            Ok(exit::OK)
        }
        Command::Export {
            format: ExportFormat::Card { spec, out: file },
        } => {
            let id = load(cli)?;
            let card = export::card(&id, spec, cli.project.as_deref())?;
            let text = serde_json::to_string_pretty(&card)?;
            match file {
                Some(f) => {
                    std::fs::write(f, format!("{text}\n"))?;
                    out.say(format!("wrote {}", f.display()));
                }
                None => println!("{text}"),
            }
            Ok(exit::OK)
        }
        Command::Export {
            format: ExportFormat::Skill { out: dir },
        } => {
            let id = load(cli)?;
            let path = export::skill(&id, dir)?;
            if out.json {
                Envelope::ok(json!({ "path": path })).print();
            } else {
                out.say(format!("wrote {path}"));
            }
            Ok(exit::OK)
        }
        Command::Export {
            format: ExportFormat::Soulspec { out: dir },
        } => {
            let id = load(cli)?;
            let report = export::soulspec(&id, dir)?;
            if out.json {
                let warnings = report.warnings.iter().map(|w| json!(w)).collect();
                Envelope {
                    ok: true,
                    data: json!({ "dir": dir, "files": report.files }),
                    warnings,
                    errors: vec![],
                }
                .print();
            } else {
                for w in &report.warnings {
                    eprintln!("warning: {w}");
                }
                for f in &report.files {
                    out.say(format!("wrote {f}"));
                }
            }
            Ok(exit::OK)
        }
        Command::Review {
            action: Some(ReviewAction::Done { through }),
        } => {
            let id = load(cli)?;
            let review = review::done(&id, through.as_deref())?;
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
    Done {
        /// Last log day covered (default: newest log date)
        #[arg(long)]
        through: Option<String>,
    },
}

#[derive(Subcommand)]
enum SoulAction {
    /// Append a soul change with its evidence
    Note {
        /// Soul file, for example SOUL.md
        file: String,
        #[arg(long)]
        change: String,
        /// Evidence entry name or [[link]] (repeatable)
        #[arg(long)]
        evidence: Vec<String>,
    },
    /// Print the soul history, optionally for one file
    History { file: Option<String> },
}

#[derive(Subcommand)]
enum ExportFormat {
    /// Character Card JSON (V3 with V2-compatible fields, or V2)
    Card {
        #[arg(long, default_value = "v3", value_parser = ["v2", "v3"])]
        spec: String,
        /// Write to a file instead of stdout
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// SKILL.md wrapper that loads the identity with owo
    Skill {
        #[arg(long)]
        out: PathBuf,
    },
    /// SoulSpec v0.5 package folder
    Soulspec {
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
enum SyncAction {
    /// Read the transcript and, when a run is due, write the slice and the ingest prompt
    Start {
        #[arg(long)]
        session: String,
        #[arg(long)]
        transcript: PathBuf,
        #[arg(long, default_value = "jsonl", value_parser = clap::builder::PossibleValuesParser::new(sync::FORMATS))]
        format: String,
        /// Take any new turns now (session end)
        #[arg(long)]
        now: bool,
        /// New user turns per run (default: identity.json sync_every, else 6)
        #[arg(long)]
        every: Option<usize>,
    },
    /// Move the bookmark after a successful ingest run
    Done {
        #[arg(long)]
        session: String,
        #[arg(long)]
        until: usize,
    },
}

#[derive(Subcommand)]
enum MigrateFrom {
    /// v1 identity folder (with references/) into <out> = <root>/<slug>
    V1 {
        path: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
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
