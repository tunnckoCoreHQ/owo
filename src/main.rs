//! owo: one world, one identity. CLI for living agent identities.

mod config;
mod create;
mod identity;
mod index;
mod lint;
mod markdown;
mod output;
mod slug;
mod state;
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
