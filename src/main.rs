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
use chrono::NaiveDate;
use clap::{CommandFactory, Parser, Subcommand};
use output::{exit, Envelope, Level};
use serde_json::json;
use std::path::{Path, PathBuf};
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
    /// Project slug (else OWO_PROJECT, .owo.toml); search and export card filter only on this flag
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
    /// Write the full text of one identity file (create or replace)
    Write {
        /// Identity-relative path under soul/, life/ or work/, ending in .md
        path: String,
        /// The file text; read from stdin when left out
        #[arg(long, allow_hyphen_values = true)]
        content: Option<String>,
    },
    /// Create an entry from its template at its deterministic path
    Add {
        /// Entry type
        #[arg(value_name = "TYPE", value_parser = clap::builder::PossibleValuesParser::new(templates::ENTRY_TEMPLATES.iter().map(|(n, _)| *n)))]
        kind: String,
        /// Title (a log without one is titled by its date)
        title: Vec<String>,
        /// Date for dated entries (default: today, local time)
        #[arg(long, value_parser = state::parse_day)]
        date: Option<NaiveDate>,
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
        #[arg(long = "rule", value_parser = clap::builder::PossibleValuesParser::new(lint::rules()))]
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
    /// List templates, or show one (built in, or overridden in <root>/.templates/)
    Templates {
        #[command(subcommand)]
        action: Option<TemplatesAction>,
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
        #[arg(long, default_value_t = 14, value_parser = clap::value_parser!(i64).range(0..=36500))]
        days: i64,
    },
    /// Files updated in the last N days
    Recent {
        #[arg(long, default_value_t = 7, value_parser = clap::value_parser!(i64).range(0..=36500))]
        days: i64,
    },
    /// BM25 search over titles, aliases, summaries and bodies
    Search {
        query: Vec<String>,
        /// Limit to soul, life or work
        #[arg(long, visible_alias = "in", value_parser = ["soul", "life", "work"])]
        scope: Option<String>,
        /// Limit to entry types (repeatable)
        #[arg(long = "type")]
        types: Vec<String>,
        /// Only entries dated (or updated) on or after YYYY-MM-DD
        #[arg(long, value_parser = state::parse_day)]
        since: Option<NaiveDate>,
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
    /// Serve every command as an MCP tool, over stdio or HTTP with --remote
    Mcp {
        /// Serve over HTTP instead of stdio, on ADDR (default 127.0.0.1); prints the URL
        #[arg(long, value_name = "ADDR", num_args = 0..=1, default_missing_value = "127.0.0.1")]
        remote: Option<String>,
        /// Port for --remote (default 4224, or the next free one)
        #[arg(long, requires = "remote")]
        port: Option<u16>,
    },
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
    let r = config::resolve(&inputs).map_err(|e| ConfigError(format!("{e:#}")))?;
    if let config::Source::ProjectFile(file) = &r.root_source {
        if file.parent() != Some(inputs.cwd.as_path()) {
            eprintln!(
                "owo: warning: the root comes from {} in a parent folder",
                file.display()
            );
        }
    }
    Ok(r)
}

fn need_identity(r: &config::Resolved) -> Result<String> {
    let slug = r.identity.clone().ok_or_else(|| {
        ConfigError(format!(
            "no identity selected in {}: pass -i <slug> or set OWO_IDENTITY",
            r.root.display()
        ))
    })?;
    if !slug::is_slug(&slug) {
        return Err(ConfigError(format!(
            "invalid identity {slug:?}: use lowercase letters, digits and single hyphens"
        ))
        .into());
    }
    if !r.root.join(&slug).is_dir() {
        return Err(ConfigError(format!(
            "identity folder not found: {}; `owo list` shows the identities in this root",
            r.root.join(&slug).display()
        ))
        .into());
    }
    Ok(slug)
}

/// Checks that the selected project is a slug with a `project.md` in the identity folder.
fn need_project<'a>(dir: &Path, project: Option<&'a str>) -> Result<Option<&'a str>> {
    let Some(p) = project else {
        return Ok(None);
    };
    if !slug::is_slug(p) {
        return Err(ConfigError(format!(
            "invalid project {p:?}: use lowercase letters, digits and single hyphens"
        ))
        .into());
    }
    if !dir.join(format!("work/projects/{p}/project.md")).is_file() {
        return Err(ConfigError(format!(
            "project not found: work/projects/{p}/project.md (run `owo add project`)"
        ))
        .into());
    }
    Ok(Some(p))
}

fn run(cli: &Cli, out: &Out) -> Result<i32> {
    match &cli.command {
        Command::Slug { text } => {
            let value = slug::slugify(&text.join(" "));
            let ok = !value.is_empty();
            if out.json {
                Envelope::new(ok, json!({ "slug": value })).print();
            } else {
                println!("{value}");
            }
            Ok(exit::of(ok))
        }
        Command::Templates { action } => templates_cmd(cli, action.as_ref(), out),
        Command::Mcp { remote, port } => {
            let d = mcp::Defaults {
                root: cli.root.clone(),
                identity: cli.identity.clone(),
                project: cli.project.clone(),
            };
            match remote {
                Some(addr) => mcp::serve_http(addr, *port, &d)?,
                None => mcp::serve(&d)?,
            }
            Ok(exit::OK)
        }
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
                    let last = row.last_activity.as_deref().map(state::day).unwrap_or("-");
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
            let (_, id) = load(cli)?;
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
            let ok = errors == 0;
            let slug = &id.slug;
            if out.json {
                let split = |level: Level| {
                    found
                        .iter()
                        .filter(|d| d.level == level)
                        .map(|d| serde_json::to_value(d).expect("serializes"))
                        .collect::<Vec<_>>()
                };
                Envelope::new(ok, json!({ "identity": slug, "infos": split(Level::Info) }))
                    .with_warnings(split(Level::Warning))
                    .with_errors(split(Level::Error))
                    .print();
            } else {
                for d in found
                    .iter()
                    .filter(|d| !out.quiet || d.level == Level::Error)
                {
                    println!("{}", d.render());
                }
                out.say(format!("{slug}: {errors} errors, {warnings} warnings"));
            }
            Ok(exit::of(ok))
        }
        Command::Write { path, content } => {
            let (_, id) = load(cli)?;
            let text = match content {
                Some(c) => c.clone(),
                None => std::io::read_to_string(std::io::stdin())?,
            };
            let created = add::write(&id, path, &text)?;
            if out.json {
                Envelope::ok(json!({ "path": path, "created": created })).print();
            } else {
                println!("{path}");
            }
            Ok(exit::OK)
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
            if title.trim().is_empty() && kind != "log" {
                Cli::command()
                    .error(
                        clap::error::ErrorKind::MissingRequiredArgument,
                        format!("`owo add {kind}` needs a title"),
                    )
                    .exit();
            }
            // Only a note lives under a project; other types ignore the working project.
            let project = if kind == "note" {
                need_project(&id.dir, r.project.as_deref())?
            } else {
                None
            };
            let req = add::Request {
                kind,
                title: &title,
                date: *date,
                parent: parent.as_deref(),
                project,
                aliases,
                kb: kb.as_deref(),
            };
            let (path, created) = add::add(&id, &req)?;
            let ok = created || kind == "log";
            if out.json {
                Envelope::new(ok, json!({ "path": path, "created": created }))
                    .with_errors((!ok).then(|| format!("exists: {path}")))
                    .print();
            } else if created {
                println!("{path}");
            } else {
                println!("exists {path}");
            }
            Ok(exit::of(ok))
        }
        Command::Stamp { check } => {
            let (_, id) = load(cli)?;
            let report = stamp::run(&id, *check)?;
            let clean = report.is_clean();
            let ok = !*check || clean;
            if out.json {
                Envelope::new(ok, serde_json::to_value(&report)?).print();
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
            Ok(exit::of(ok))
        }
        Command::Index { check } => {
            let (_, id) = load(cli)?;
            let paths = if *check {
                index::stale(&id)
            } else {
                index::write(&id)?
            };
            let ok = !*check || paths.is_empty();
            if out.json {
                let key = if *check { "stale" } else { "written" };
                Envelope::new(ok, json!({ key: paths })).print();
            } else {
                for p in &paths {
                    out.say(format!("{}{p}", if *check { "stale " } else { "wrote " }));
                }
                if paths.is_empty() {
                    out.say("generated files are up to date");
                }
            }
            Ok(exit::of(ok))
        }
        Command::Resolve { name, kind } => {
            let (_, id) = load(cli)?;
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
                        Envelope::new(false, json!({ "candidates": hits }))
                            .with_errors([msg])
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
            let (_, id) = load(cli)?;
            let joined = name.join(" ");
            let target = joined.trim_start_matches("[[").trim_end_matches("]]");
            if let Some(shown) = find::show_kb(&id, target, section.as_deref()) {
                let (file, body) = shown?;
                if out.json {
                    Envelope::ok(json!({ "path": file, "text": body })).print();
                } else {
                    print!("{body}");
                }
                return Ok(exit::OK);
            }
            let i = resolve_one(&id, &joined)?;
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
            let (_, id) = load(cli)?;
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
            let (r, id) = load(cli)?;
            let project = need_project(&id.dir, r.project.as_deref())?;
            let req = context::Request {
                voice: *voice,
                look: *look,
                project,
                config_kb: r.kb.as_ref().map(PathBuf::from),
                with,
                notes: *notes,
            };
            let ctx = context::build(&id, &req)?;
            if let (Some(p), false) = (project, *no_touch) {
                context::touch_project(&id, p)?;
            }
            if out.json {
                Envelope::ok(serde_json::to_value(&ctx)?)
                    .with_warnings(ctx.warnings.clone())
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
            let (r, id) = load(cli)?;
            let mut s = status::compute(&id, *days, 5);
            if let Some(p) = need_project(&id.dir, r.project.as_deref())? {
                s.project = Some(status::project(&id, p));
            }
            if out.json {
                Envelope::ok(serde_json::to_value(&s)?).print();
            } else {
                print!("{}", status::render(&s));
            }
            Ok(exit::OK)
        }
        Command::Recent { days } => {
            let (_, id) = load(cli)?;
            let list = status::recent(&id, *days);
            if out.json {
                Envelope::ok(json!({ "days": days, "files": list })).print();
            } else {
                if list.is_empty() {
                    eprintln!("owo: nothing stamped in the last {days} days");
                }
                for f in &list {
                    println!("{}  {}", state::day(&f.updated), f.path);
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
            if out.json {
                Envelope::new(ok, json!({ "hits": hits }))
                    .with_errors((!ok).then_some("no hits"))
                    .print();
            } else {
                if hits.is_empty() {
                    eprintln!("owo: no hits");
                }
                for h in &hits {
                    println!("{:>6.2}  {}  {}", h.score, h.path, h.title);
                    if let (Some(l), Some(s)) = (h.line, &h.snippet) {
                        println!("        {l}: {s}");
                    }
                }
            }
            Ok(exit::of(ok))
        }
        Command::Sync {
            action:
                action @ (SyncAction::Start {
                    session,
                    now,
                    every,
                    ..
                }
                | SyncAction::Push {
                    session,
                    now,
                    every,
                    ..
                }),
        } => {
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
            let mut req = sync::StartRequest {
                root: &r.root,
                slug: &slug,
                session,
                transcript: Path::new(""),
                format: "jsonl",
                now: *now,
                every,
                project,
            };
            let s = match action {
                SyncAction::Start {
                    transcript, format, ..
                } => {
                    req.transcript = transcript;
                    req.format = format;
                    sync::start(&req)?
                }
                SyncAction::Push { turns, .. } => {
                    let turns = match turns {
                        Some(t) => t.clone(),
                        None => std::io::read_to_string(std::io::stdin())?,
                    };
                    sync::push(&req, &turns)?
                }
                SyncAction::Done { .. } => unreachable!("matched above"),
            };
            if let Some(n) = s.added.filter(|_| !out.json) {
                out.say(format!("added {n}"));
            }
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
                    "not due: {} new user turns (every {}){}",
                    s.new_user_turns,
                    s.every,
                    s.pending
                        .as_deref()
                        .map(|p| format!("; a run is pending ({p}) until `sync done`"))
                        .unwrap_or_default()
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
            let (_, id) = load(cli)?;
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
            let (_, id) = load(cli)?;
            let note = review::soul_note(&id, file, change, evidence)?;
            if out.json {
                Envelope::ok(note.record)
                    .with_warnings(note.warning)
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
            let (_, id) = load(cli)?;
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
            let (_, id) = load(cli)?;
            let c = changes::changes(&id, since.as_deref())?;
            if out.json {
                Envelope::ok(serde_json::to_value(&c)?).print();
            } else {
                print!("{}", changes::render(&c));
            }
            Ok(exit::OK)
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
            let warnings: Vec<serde_json::Value> = ["first_mes", "mes_example"]
                .iter()
                .filter(|k| {
                    card["data"][**k]
                        .as_str()
                        .is_none_or(|s| s.trim().is_empty())
                })
                .map(|k| json!(format!("{k} is empty; fill soul/examples/")))
                .collect();
            if !out.json {
                for w in &warnings {
                    eprintln!("owo: warning: {}", w.as_str().unwrap_or_default());
                }
            }
            let Some(f) = file else {
                if out.json {
                    Envelope::ok(card).with_warnings(warnings).print();
                } else {
                    println!("{}", serde_json::to_string_pretty(&card)?);
                }
                return Ok(exit::OK);
            };
            export::write_card(&id, spec, project, scenario, f, png.as_deref())?;
            if out.json {
                Envelope::ok(json!({ "path": f }))
                    .with_warnings(warnings)
                    .print();
            } else {
                out.say(format!("wrote {}", f.display()));
            }
            Ok(exit::OK)
        }
        Command::Export {
            format: ExportFormat::Skill { out: dir },
        } => {
            let (_, id) = load(cli)?;
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
            let (_, id) = load(cli)?;
            let report = export::soulspec(&id, dir)?;
            if out.json {
                Envelope::ok(json!({ "dir": dir, "files": report.files }))
                    .with_warnings(report.warnings)
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
            let (_, id) = load(cli)?;
            let review = review::done(&id, *through)?;
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
enum TemplatesAction {
    /// List template names and where each comes from
    List,
    /// Print a template as `owo new` and `owo add` would use it
    Show {
        /// Template name from `owo templates list`
        name: String,
    },
}
#[derive(Subcommand)]
enum ReviewAction {
    /// Record a finished review (dates and the MEMORY.md hash)
    Done {
        /// Last story day covered (default: newest log, event or journal date)
        #[arg(long, value_parser = state::parse_day)]
        through: Option<NaiveDate>,
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
        /// Embed the card in this PNG avatar (needs --out)
        #[arg(long, requires = "out")]
        png: Option<PathBuf>,
        /// Event or location whose summary becomes the scenario
        #[arg(long)]
        scenario: Option<String>,
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
        /// Session id from the host (letters, digits, -, _ or .)
        #[arg(long)]
        session: String,
        /// Transcript file the host keeps for this session
        #[arg(long)]
        transcript: PathBuf,
        /// Transcript format; jsonl lines are {"role": ..., "text": ...}
        #[arg(long, default_value = "jsonl", value_parser = clap::builder::PossibleValuesParser::new(sync::FORMATS))]
        format: String,
        /// Take any new turns now (session end)
        #[arg(long)]
        now: bool,
        /// New user turns per run (default: identity.json sync_every, else 6)
        #[arg(long)]
        every: Option<usize>,
    },
    /// Store turns sent by the host, then run sync start on them (for hosts without a local transcript)
    Push {
        /// Session id from the host (letters, digits, -, _ or .)
        #[arg(long)]
        session: String,
        /// JSON array of {"id", "role", "text"}; turns whose id is stored already are skipped (default: stdin)
        #[arg(long, allow_hyphen_values = true)]
        turns: Option<String>,
        /// Take any new turns now (session end)
        #[arg(long)]
        now: bool,
        /// New user turns per run (default: identity.json sync_every, else 6)
        #[arg(long)]
        every: Option<usize>,
    },
    /// Move the bookmark after a successful ingest run
    Done {
        /// Session id given to sync start
        #[arg(long)]
        session: String,
        /// Message index the ingest run reached (the until from sync start)
        #[arg(long)]
        until: usize,
    },
}

#[derive(Subcommand)]
enum MigrateFrom {
    /// v1 identity folder (with references/) into <out> = <root>/<slug>
    V1 {
        /// The v1 identity folder
        path: PathBuf,
        #[arg(long)]
        /// New identity folder, <root>/<slug>
        out: PathBuf,
    },
}

fn load(cli: &Cli) -> Result<(config::Resolved, identity::Identity)> {
    let r = resolved(cli)?;
    let id = identity::Identity::load(&r.root, &need_identity(&r)?)?;
    Ok((r, id))
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

fn templates_cmd(cli: &Cli, action: Option<&TemplatesAction>, out: &Out) -> Result<i32> {
    let root = resolved(cli)?.root;
    let all = templates::all();
    match action {
        None | Some(TemplatesAction::List) => {
            let rows: Vec<_> = all
                .iter()
                .map(|(name, file, _)| (*name, templates::override_path(&root, file)))
                .collect();
            if out.json {
                let list: Vec<_> = rows
                    .iter()
                    .map(|(name, path)| json!({ "name": name, "override": path }))
                    .collect();
                Envelope::ok(json!({ "templates": list })).print();
            } else {
                for (name, path) in rows {
                    match path {
                        Some(p) => println!("{name:<28} override {}", p.display()),
                        None => println!("{name:<28} built-in"),
                    }
                }
            }
            Ok(exit::OK)
        }
        Some(TemplatesAction::Show { name }) => {
            let Some((name, file, builtin)) = all.iter().find(|(n, _, _)| n == name) else {
                anyhow::bail!("unknown template {name:?}; run `owo templates` for the list");
            };
            let text = templates::get(&root, file, builtin);
            if out.json {
                let path = templates::override_path(&root, file);
                Envelope::ok(json!({ "name": name, "override": path, "text": text })).print();
            } else {
                print!("{text}");
            }
            Ok(exit::OK)
        }
    }
}
