use clap::builder::PossibleValuesParser;
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};
use owo_core::{Args, Field, Operation, Select, Type};
use serde_json::{Map, Value};
use std::path::PathBuf;

const GROUPS: &[(&str, &str)] = &[
    ("export", "Export the identity to another format"),
    (
        "sync",
        "Session sync: slice a host transcript into sources/ for ingest",
    ),
    ("migrate", "Migrate an identity from an older layout"),
];

pub struct Global {
    pub select: Select,
    pub json: bool,
    pub quiet: bool,
}

impl Global {
    pub fn of(m: &ArgMatches) -> Self {
        Self {
            select: Select {
                root: m.get_one::<PathBuf>("root").cloned(),
                identity: m.get_one::<String>("identity").cloned(),
                project: m.get_one::<String>("project").cloned(),
            },
            json: m.get_flag("json"),
            quiet: m.get_flag("quiet"),
        }
    }
}

pub fn command(ops: &[Operation]) -> Command {
    let mut root = Command::new("owo")
        .version(env!("CARGO_PKG_VERSION"))
        .about("One world, one identity: manage living agent identities")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .args(globals());
    for op in ops {
        let cmd = Command::new(op.path[op.path.len() - 1])
            .about(op.about)
            .args(op.fields.iter().map(|f| arg(op, f)));
        root = match op.path {
            [_] => root.subcommand(cmd),
            [group, _] if root.find_subcommand(group).is_some() => {
                root.mut_subcommand(group, |g| g.subcommand(cmd))
            }
            [group, _] => {
                let about = GROUPS.iter().find(|(g, _)| g == group).map(|(_, a)| *a);
                root.subcommand(
                    Command::new(*group)
                        .about(about.unwrap_or_default())
                        .subcommand_required(true)
                        .arg_required_else_help(true)
                        .subcommand(cmd),
                )
            }
            _ => unreachable!("an operation path has one or two words"),
        };
    }
    root.subcommand(mcp()).subcommand(acp())
}

fn globals() -> [Arg; 5] {
    [
        Arg::new("root")
            .long("root")
            .value_name("ROOT")
            .global(true)
            .value_parser(value_parser!(PathBuf))
            .help("Identities root (else OWO_ROOT, .owo.toml, XDG default)"),
        Arg::new("identity")
            .short('i')
            .long("identity")
            .value_name("IDENTITY")
            .global(true)
            .help("Identity slug (else OWO_IDENTITY, .owo.toml, owo.toml in the root, the only identity)"),
        Arg::new("project")
            .long("project")
            .value_name("PROJECT")
            .global(true)
            .help("Project slug (else OWO_PROJECT, .owo.toml); search and export persona-card filter only on this flag"),
        Arg::new("json")
            .long("json")
            .global(true)
            .action(ArgAction::SetTrue)
            .help("Print the JSON envelope instead of text"),
        Arg::new("quiet")
            .short('q')
            .long("quiet")
            .global(true)
            .action(ArgAction::SetTrue)
            .help("Print only problems"),
    ]
}

fn mcp() -> Command {
    Command::new("mcp")
        .about("Serve every command as an MCP tool, over stdio or HTTP with --remote")
        .arg(
            Arg::new("remote")
                .long("remote")
                .value_name("ADDR")
                .num_args(0..=1)
                .default_missing_value("127.0.0.1")
                .help(
                    "Serve over HTTP instead of stdio, on ADDR (default 127.0.0.1); prints the URL",
                ),
        )
        .arg(
            Arg::new("port")
                .long("port")
                .value_name("PORT")
                .requires("remote")
                .value_parser(value_parser!(u16))
                .help("Port for --remote (default 4224, or the next free one)"),
        )
}

fn acp() -> Command {
    Command::new("acp")
        .about("Run an ACP agent behind a pass-through proxy that stores turns and injects the identity")
        .arg(
            Arg::new("inject")
                .long("inject")
                .action(ArgAction::SetTrue)
                .help("Put the identity context in the prompt, and add no owo MCP server (needs an identity in the root)"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue)
                .help("Store no turns; print `owo: would write <path>` on stderr for each file a store would write"),
        )
        .arg(
            Arg::new("agent")
                .last(true)
                .required(true)
                .num_args(1..)
                .value_name("AGENT")
                .help("The agent command and its arguments, after --"),
        )
}

fn help(op: &Operation, f: &Field) -> &'static str {
    match (op.path, f.name) {
        (["stamp"], "check") => "Report drift and exit 1 instead of writing",
        (["index"], "check") => "Report stale generated files and exit 1 instead of writing",
        (["write"], "content") => "The file text; read from stdin when left out",
        (["templates", "show"], "name") => "Template name from `owo templates list`",
        (["links"], "name") => "Entry name, alias, path or [[link]] (not needed with --orphans)",
        (["export", "persona-card"], "png") => "Embed the card in this PNG avatar (needs --out)",
        (["sync", "push"], "turns") => "JSON array of {\"id\", \"role\", \"text\"}; id is the turn id, the host's id for that message, and a stored turn id is skipped (default: stdin)",
        _ => f.help,
    }
}

fn flag(name: &str) -> String {
    let mut flag = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            flag.push('-');
        }
        flag.push(c.to_ascii_lowercase());
    }
    flag
}

fn arg(op: &Operation, f: &Field) -> Arg {
    let mut a = Arg::new(f.name).help(help(op, f));
    if !f.positional {
        a = a.long(flag(f.name));
    }
    if f.ty == Type::Switch {
        return a.action(ArgAction::SetTrue);
    }
    a = a
        .action(if f.many {
            ArgAction::Append
        } else {
            ArgAction::Set
        })
        .required(f.required)
        .allow_hyphen_values(f.hyphen);
    if f.many && f.positional {
        a = a.num_args(1..);
    }
    a = match (&f.ty, f.range) {
        (Type::Int, Some((min, max))) => a.value_parser(value_parser!(i64).range(min..=max)),
        (Type::Int, None) => a.value_parser(value_parser!(usize)),
        (Type::Day, _) => a.value_parser(owo_core::parse_day),
        (Type::Path, _) => a.value_parser(value_parser!(PathBuf)),
        (Type::Choice(values), _) => a.value_parser(PossibleValuesParser::new(values.clone())),
        _ => a,
    };
    if let Some(d) = &f.default {
        a = a.default_value(d.clone());
    }
    a = a.value_name(
        f.value_name
            .map_or_else(|| f.name.to_uppercase(), str::to_string),
    );
    if let Some(other) = f.conflicts {
        a = a.conflicts_with(other);
    }
    if let Some(other) = f.requires {
        a = a.requires(other);
    }
    a
}

pub fn args(op: &Operation, m: &ArgMatches) -> Map<String, Value> {
    let mut map = Map::new();
    for f in &op.fields {
        if f.ty == Type::Switch {
            map.insert(f.name.into(), m.get_flag(f.name).into());
            continue;
        }
        let Some(raw) = m.get_raw(f.name) else {
            continue;
        };
        let mut values: Vec<Value> = raw
            .map(|v| v.to_string_lossy().into_owned().into())
            .collect();
        let value = if f.many {
            Value::Array(values)
        } else {
            values.swap_remove(0)
        };
        map.insert(f.name.into(), value);
    }
    map
}

pub fn with_paths(op: &Operation, m: &ArgMatches, mut args: Args) -> Args {
    for f in op.fields.iter().filter(|f| f.ty == Type::Path) {
        if let Some(p) = m.get_one::<PathBuf>(f.name) {
            args = args.with_path(f.name, p.clone());
        }
    }
    args
}
