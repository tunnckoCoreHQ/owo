//! `owo mcp`: a stdio MCP server. Each tool runs this same binary with the matching
//! command line, so a tool call and a CLI call share one code path and one output.
//! Messages are newline-delimited JSON-RPC 2.0.

use anyhow::Result;
use serde_json::{json, Map, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::Command;

const PROTOCOL: &str = "2025-06-18";

enum Arg {
    /// A value after `--`, in table order.
    Positional,
    /// `--flag <value>`, repeated for arrays.
    Flag(&'static str),
    /// `--flag` when true.
    Switch(&'static str),
}

struct Param {
    name: &'static str,
    /// JSON schema type: string, integer, boolean, or array (of strings).
    kind: &'static str,
    arg: Arg,
    required: bool,
    about: &'static str,
}

struct Tool {
    name: &'static str,
    about: &'static str,
    /// Subcommand and fixed flags.
    command: &'static [&'static str],
    /// Run with `--json` and return the envelope; otherwise return the text output.
    json: bool,
    params: &'static [Param],
}

const fn p(
    name: &'static str,
    kind: &'static str,
    arg: Arg,
    required: bool,
    about: &'static str,
) -> Param {
    Param {
        name,
        kind,
        arg,
        required,
        about,
    }
}

use Arg::{Flag, Positional, Switch};

const TOOLS: &[Tool] = &[
    Tool {
        name: "context",
        about: "Load the identity: core soul files, the Now block, and optional voice, look and project tiers, as text.",
        command: &["context", "--format", "text"],
        json: false,
        params: &[
            p("voice", "boolean", Switch("--voice"), false, "Add soul/VOICE.md"),
            p("look", "boolean", Switch("--look"), false, "Add soul/LOOK.md"),
            p("with", "array", Flag("--with"), false, "Extra entries to include, by name"),
            p("notes", "integer", Flag("--notes"), false, "Recent project notes to include (default 3)"),
            p("no_touch", "boolean", Switch("--no-touch"), false, "Leave work/state.json untouched"),
        ],
    },
    Tool {
        name: "resolve",
        about: "Resolve a name (title, alias, slug or path) to an entry, the way wiki-links resolve.",
        command: &["resolve"],
        json: true,
        params: &[
            p("name", "string", Positional, true, "Name to resolve"),
            p("type", "string", Flag("--type"), false, "Limit to one entry type"),
        ],
    },
    Tool {
        name: "search",
        about: "BM25 search over titles, aliases, summaries and bodies.",
        command: &["search"],
        json: true,
        params: &[
            p("query", "string", Positional, true, "Search words"),
            p("in", "string", Flag("--in"), false, "Limit to soul, life or work"),
            p("type", "array", Flag("--type"), false, "Limit to entry types"),
            p("since", "string", Flag("--since"), false, "Only entries dated on or after YYYY-MM-DD"),
            p("limit", "integer", Flag("--limit"), false, "Maximum hits (default 10)"),
        ],
    },
    Tool {
        name: "show",
        about: "Print an entry, or one section of it.",
        command: &["show"],
        json: false,
        params: &[
            p("name", "string", Positional, true, "Entry name, or kb:<project>:<path>#<Section>"),
            p("section", "string", Flag("--section"), false, "Section heading"),
        ],
    },
    Tool {
        name: "links",
        about: "Backlinks and outlinks of an entry, or every orphan.",
        command: &["links"],
        json: true,
        params: &[
            p("name", "string", Positional, false, "Entry name (omit with orphans)"),
            p("in", "boolean", Switch("--in"), false, "Only links pointing at the entry"),
            p("out", "boolean", Switch("--out"), false, "Only links going out of the entry"),
            p("orphans", "boolean", Switch("--orphans"), false, "List entries nothing links to"),
        ],
    },
    Tool {
        name: "state",
        about: "Derived current state: activity, recent days and events, open threads, review status.",
        command: &["state"],
        json: true,
        params: &[p("days", "integer", Flag("--days"), false, "Window for recent events (default 14)")],
    },
    Tool {
        name: "add",
        about: "Create an entry from its template at its deterministic path; returns the path.",
        command: &["add"],
        json: true,
        params: &[
            p("type", "string", Positional, true, "Entry type, for example character, event, log, note"),
            p("title", "string", Positional, false, "Title (a log without one is titled by its date)"),
            p("date", "string", Flag("--date"), false, "YYYY-MM-DD for dated entries"),
            p("parent", "string", Flag("--parent"), false, "Parent location for location-detail"),
            p("alias", "array", Flag("--alias"), false, "Aliases"),
            p("kb", "string", Flag("--kb"), false, "KB path for a project"),
        ],
    },
    Tool {
        name: "stamp",
        about: "Refresh state.json files from content hashes.",
        command: &["stamp"],
        json: true,
        params: &[p("check", "boolean", Switch("--check"), false, "Report drift instead of writing")],
    },
    Tool {
        name: "index",
        about: "Regenerate _index.md files and life/timeline.md.",
        command: &["index"],
        json: true,
        params: &[p("check", "boolean", Switch("--check"), false, "Report stale files instead of writing")],
    },
    Tool {
        name: "lint",
        about: "Validate the identity.",
        command: &["lint"],
        json: true,
        params: &[
            p("rule", "array", Flag("--rule"), false, "Run only these rules"),
            p("changed", "boolean", Switch("--changed"), false, "Report only changed files"),
        ],
    },
];

/// Defaults the server was started with (`owo --root R -i thea mcp`).
pub struct Defaults {
    pub root: Option<PathBuf>,
    pub identity: Option<String>,
    pub project: Option<String>,
}

fn schema(tool: &Tool) -> Value {
    let mut props = Map::new();
    props.insert(
        "identity".into(),
        json!({ "type": "string", "description": "Identity slug (default: the server's)" }),
    );
    props.insert(
        "project".into(),
        json!({ "type": "string", "description": "Project slug" }),
    );
    for prm in tool.params {
        let mut s = json!({ "type": prm.kind, "description": prm.about });
        if prm.kind == "array" {
            s["items"] = json!({ "type": "string" });
        }
        props.insert(prm.name.into(), s);
    }
    let required: Vec<&str> = tool
        .params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name)
        .collect();
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// The command line for a tool call. Positionals go after `--`, so values never parse as flags.
fn argv(tool: &Tool, args: &Map<String, Value>, d: &Defaults) -> Result<Vec<String>, String> {
    if let Some(k) = args
        .keys()
        .find(|k| *k != "identity" && *k != "project" && !tool.params.iter().any(|p| p.name == *k))
    {
        return Err(format!("unknown argument {k:?} for {}", tool.name));
    }
    let mut out: Vec<String> = vec![];
    if let Some(root) = &d.root {
        out.extend(["--root".into(), root.display().to_string()]);
    }
    let pick = |key: &str, default: &Option<String>| {
        args.get(key).and_then(scalar).or_else(|| default.clone())
    };
    if let Some(i) = pick("identity", &d.identity) {
        out.extend(["-i".into(), i]);
    }
    if let Some(p) = pick("project", &d.project) {
        out.extend(["--project".into(), p]);
    }
    if tool.json {
        out.push("--json".into());
    }
    out.extend(tool.command.iter().map(|s| s.to_string()));
    let mut positionals = vec![];
    for prm in tool.params {
        let Some(v) = args.get(prm.name).filter(|v| !v.is_null()) else {
            if prm.required {
                return Err(format!("missing argument {:?}", prm.name));
            }
            continue;
        };
        let values: Vec<String> = match v {
            Value::Array(items) => items.iter().filter_map(scalar).collect(),
            other => scalar(other).into_iter().collect(),
        };
        match prm.arg {
            Positional => positionals.extend(values),
            Flag(f) => {
                for value in values {
                    out.extend([f.to_string(), value]);
                }
            }
            Switch(f) => {
                if v.as_bool() == Some(true) {
                    out.push(f.to_string());
                }
            }
        }
    }
    if !positionals.is_empty() {
        out.push("--".into());
        out.extend(positionals);
    }
    Ok(out)
}

fn call(params: &Value, d: &Defaults) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let empty = Map::new();
    let args = params["arguments"].as_object().unwrap_or(&empty);
    let result = |text: String, error: bool| json!({ "content": [{ "type": "text", "text": text }], "isError": error });
    let Some(tool) = TOOLS.iter().find(|t| t.name == name) else {
        return result(format!("unknown tool {name:?}"), true);
    };
    let argv = match argv(tool, args, d) {
        Ok(a) => a,
        Err(e) => return result(e, true),
    };
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("owo"));
    match Command::new(exe).args(&argv).output() {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
            let text = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
                (true, _) => stderr,
                (false, true) => stdout,
                (false, false) => format!("{stdout}\n{stderr}"),
            };
            result(text, !o.status.success())
        }
        Err(e) => result(format!("could not run owo: {e}"), true),
    }
}

fn respond(id: &Value, method: &str, params: &Value, d: &Defaults) -> Option<Value> {
    let ok = |result: Value| Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    match method {
        "initialize" => ok(json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "owo", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "owo manages one identity: soul/, life/ and work/. Start with `context`, read more with `resolve`, `search`, `show` and `links`, write new entries with `add`, then run `stamp`, `index` and `lint`.",
        })),
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({
            "tools": TOOLS
                .iter()
                .map(|t| json!({ "name": t.name, "description": t.about, "inputSchema": schema(t) }))
                .collect::<Vec<_>>()
        })),
        "tools/call" => ok(call(params, d)),
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("method not found: {method}") },
        })),
    }
}

/// Serves MCP on stdin and stdout until stdin closes.
pub fn serve(d: &Defaults) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => {
                let id = msg.get("id").cloned().unwrap_or(Value::Null);
                let method = msg["method"].as_str().unwrap_or_default();
                // Notifications (no id) and responses get no reply.
                if msg.get("id").is_none() || msg.get("method").is_none() {
                    continue;
                }
                respond(&id, method, &msg["params"], d)
            }
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") },
            })),
        };
        if let Some(r) = reply {
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
