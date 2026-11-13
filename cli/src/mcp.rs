//! The MCP server, a consumer of the library. Each tool is a command, read from the clap
//! definitions. A tool call maps its JSON arguments to the library request of its command, with
//! the library's defaults for the arguments it leaves out, and renders the result with `render`,
//! as the command does. The library checks the rules on arguments. Messages are JSON-RPC 2.0:
//! one per line with `serve`, or one per POST to `/mcp` with `serve_http`. `reply` answers one
//! message.

use crate::output::{Out, Output};
use crate::render::{self, load, open};
use anyhow::{Context, Result};
use owo::{Env, Select};
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const PROTOCOL: &str = "2025-06-18";
/// Versions whose messages this server speaks; `initialize` echoes one of these, else `PROTOCOL`.
const SUPPORTED: &[&str] = &[PROTOCOL, "2025-03-26", "2024-11-05"];

struct Param {
    name: String,
    /// JSON schema type: string, integer, boolean, or array (of strings, or of objects).
    kind: &'static str,
    /// An array of objects (`sync push` turns), not of strings.
    objects: bool,
    required: bool,
    about: String,
    /// Allowed values, when the flag has a fixed list.
    values: Vec<String>,
}

struct Tool {
    name: String,
    about: String,
    /// Return the JSON envelope; otherwise return the text output.
    json: bool,
    params: Vec<Param>,
}

/// Every command as a tool, read from the clap definitions: `review done` becomes `review_done`.
/// A command with optional subcommands (`review`) is a tool itself too. `mcp` and `acp` are left out.
fn tools() -> Vec<Tool> {
    fn walk(cmd: &clap::Command, path: &[&str], out: &mut Vec<Tool>) {
        for sub in cmd
            .get_subcommands()
            .filter(|s| !matches!(s.get_name(), "mcp" | "acp" | "help"))
        {
            let mut path = path.to_vec();
            path.push(sub.get_name());
            if !sub.has_subcommands() || !sub.is_subcommand_required_set() {
                out.push(tool(sub, &path));
            }
            walk(sub, &path, out);
        }
    }
    let mut out = vec![];
    walk(
        &<crate::cli::Cli as clap::CommandFactory>::command(),
        &[],
        &mut out,
    );
    out
}

fn tool(cmd: &clap::Command, path: &[&str]) -> Tool {
    let name = path.join("_").replace('-', "_");
    let params = cmd
        .get_arguments()
        .filter(|a| !a.is_global_set() && !a.is_hide_set())
        .map(param)
        .collect();
    Tool {
        json: !matches!(name.as_str(), "context" | "show"),
        about: cmd.get_about().map(|a| a.to_string()).unwrap_or_default(),
        name,
        params,
    }
}

fn param(a: &clap::Arg) -> Param {
    use std::any::TypeId;
    let switch = matches!(a.get_action(), clap::ArgAction::SetTrue);
    let parser = a.get_value_parser().type_id();
    let integer = [
        TypeId::of::<usize>(),
        TypeId::of::<u64>(),
        TypeId::of::<u16>(),
        TypeId::of::<i64>(),
    ]
    .iter()
    .any(|t| parser == *t);
    // `sync push --turns` is a JSON array, so the tool takes a real array.
    let objects = a.get_id() == "turns";
    let kind = if switch {
        "boolean"
    } else if a.is_positional() {
        // A multi-word positional (a title, a query) is one string.
        "string"
    } else if objects || matches!(a.get_action(), clap::ArgAction::Append) {
        "array"
    } else if integer {
        "integer"
    } else {
        "string"
    };
    let long = a.get_long().unwrap_or(a.get_id().as_str());
    Param {
        name: long.replace('-', "_"),
        kind,
        objects,
        required: a.is_required_set(),
        about: a.get_help().map(|h| h.to_string()).unwrap_or_default(),
        values: if switch {
            vec![]
        } else {
            a.get_possible_values()
                .iter()
                .filter(|v| !v.is_hide_set())
                .map(|v| v.get_name().to_string())
                .collect()
        },
    }
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
    for prm in &tool.params {
        let mut s = json!({ "type": prm.kind, "description": prm.about });
        let values = (!prm.values.is_empty()).then(|| json!(prm.values));
        if prm.kind == "array" {
            let item = if prm.objects { "object" } else { "string" };
            s["items"] = json!({ "type": item });
            if let Some(v) = values {
                s["items"]["enum"] = v;
            }
        } else if let Some(v) = values {
            s["enum"] = v;
        }
        props.insert(prm.name.clone(), s);
    }
    let required: Vec<&str> = tool
        .params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.as_str())
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

/// The arguments of one tool call, checked against the tool's parameters.
struct Args<'a> {
    map: &'a Map<String, Value>,
}

impl<'a> Args<'a> {
    /// Checks that each argument is a parameter, each required one is there, and each value of a
    /// parameter with a fixed list is in it.
    fn new(tool: &'a Tool, map: &'a Map<String, Value>) -> Result<Self, String> {
        if let Some(k) = map.keys().find(|k| {
            *k != "identity" && *k != "project" && !tool.params.iter().any(|p| p.name == **k)
        }) {
            return Err(format!("unknown argument {k:?} for {}", tool.name));
        }
        let args = Args { map };
        for prm in &tool.params {
            if prm.required && args.get(&prm.name).is_none() {
                return Err(format!("missing argument {:?}", prm.name));
            }
            if prm.values.is_empty() {
                continue;
            }
            for v in args.strings(&prm.name) {
                if !prm.values.contains(&v) {
                    return Err(format!(
                        "invalid value {v:?} for {:?}; possible values: {}",
                        prm.name,
                        prm.values.join(", ")
                    ));
                }
            }
        }
        Ok(args)
    }

    fn get(&self, name: &str) -> Option<&'a Value> {
        self.map.get(name).filter(|v| !v.is_null())
    }

    /// The scalar values of an argument: each item of an array, or the one value.
    fn strings(&self, name: &str) -> Vec<String> {
        match self.get(name) {
            Some(Value::Array(items)) => items.iter().filter_map(scalar).collect(),
            Some(v) => scalar(v).into_iter().collect(),
            None => vec![],
        }
    }

    /// A single-value argument: a scalar, or an array of one scalar.
    fn text(&self, name: &str) -> Result<Option<String>, String> {
        let one = match self.get(name) {
            None => return Ok(None),
            Some(Value::Array(items)) if items.len() == 1 => &items[0],
            Some(v) => v,
        };
        scalar(one)
            .map(Some)
            .ok_or_else(|| format!("argument {name:?} must be a string"))
    }

    /// A required single-value argument; `Args::new` checked it is there.
    fn required(&self, name: &str) -> Result<String, String> {
        Ok(self.text(name)?.unwrap_or_default())
    }

    /// A positional argument of several words: a string, or an array of words.
    fn words(&self, name: &str) -> String {
        self.strings(name).join(" ")
    }

    fn flag(&self, name: &str) -> bool {
        self.get(name).and_then(Value::as_bool) == Some(true)
    }

    /// An integer argument: a number, or a string of one.
    fn int<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, String> {
        self.text(name)?
            .map(|s| {
                s.parse()
                    .map_err(|_| format!("argument {name:?} must be an integer, not {s:?}"))
            })
            .transpose()
    }

    /// A `YYYY-MM-DD` argument.
    fn day(&self, name: &str) -> Result<Option<chrono::NaiveDate>, String> {
        self.text(name)?
            .map(|s| owo::parse_day(&s).map_err(|e| format!("argument {name:?}: {e}")))
            .transpose()
    }

    /// A value of a fixed list, as its library type. `Args::new` checked the name.
    fn named<T>(&self, name: &str, from: fn(&str) -> Option<T>) -> Result<Option<T>, String> {
        Ok(self.text(name)?.and_then(|s| from(&s)))
    }

    /// The values of a repeatable flag with a fixed list, as their library type.
    fn all_named<T>(&self, name: &str, from: fn(&str) -> Option<T>) -> Vec<T> {
        self.strings(name).iter().filter_map(|s| from(s)).collect()
    }
}

fn result(text: String, error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": error })
}

/// Runs one tool call. A panic becomes an error result, so the server answers the next request.
fn call(tool: &Tool, params: &Value, select: &Select, env: &Env) -> Value {
    guarded(|| run_tool(tool, params, select, env))
}

fn guarded(f: impl FnOnce() -> Value) -> Value {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|panic| {
        let why = panic
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        result(format!("owo panicked: {why}"), true)
    })
}

/// Maps the tool's arguments to its library call and renders the result as the command does.
/// The text is the command's stdout, or its stderr when stdout is blank, or both joined with a
/// newline.
fn run_tool(tool: &Tool, params: &Value, select: &Select, env: &Env) -> Value {
    let empty = Map::new();
    let map = params["arguments"].as_object().unwrap_or(&empty);
    // A command line cannot hold a NUL, so no tool argument can; `turns` is a JSON document.
    let nul = map
        .iter()
        .filter(|(k, _)| *k != "turns")
        .any(|(_, v)| match v {
            Value::Array(items) => items.iter().filter_map(scalar).any(|s| s.contains('\0')),
            v => scalar(v).is_some_and(|s| s.contains('\0')),
        });
    if nul {
        return result("an argument holds a NUL character".into(), true);
    }
    let output = Args::new(tool, map).and_then(|args| {
        let pick = |key: &str, default: &Option<String>| {
            map.get(key).and_then(scalar).or_else(|| default.clone())
        };
        let select = Select {
            root: select.root.clone(),
            identity: pick("identity", &select.identity),
            project: pick("project", &select.project),
        };
        dispatch(tool, &args, &select, env)?
            .ok_or_else(|| format!("no library call for the tool {:?}", tool.name))
    });
    let out = match output {
        Ok(out) => out,
        Err(e) => return result(e, true),
    };
    let (stdout, stderr) = out.render(tool.json);
    let text = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, _) => stderr,
        (false, true) => stdout,
        (false, false) => format!("{stdout}\n{stderr}"),
    };
    result(text, out.code != 0)
}

/// The library call of one tool, or None for a tool with none. An argument error is the error
/// text.
fn dispatch(tool: &Tool, a: &Args, sel: &Select, env: &Env) -> Result<Option<Output>, String> {
    use owo::{Kind, Rule, Scope, Spec, SyncFormat};
    let out = Out {
        json: tool.json,
        quiet: false,
        warnings: vec![],
    };
    let output = match tool.name.as_str() {
        "slug" => {
            let text = a.words("text");
            render::finish(out, |out| render::slug(out, &owo::slug(&text)?))
        }
        "templates" | "templates_list" => render::finish(out, |out| {
            let root = open(sel, env, out)?;
            render::templates(out, &root.templates())
        }),
        "templates_show" => {
            let name = a.required("name")?;
            render::finish(out, |out| {
                let (tpl, text) = open(sel, env, out)?.template(&name)?;
                render::template(out, &tpl, &text)
            })
        }
        "config" => render::finish(out, |out| {
            let r = open(sel, env, out)?.config;
            render::config(out, &r)
        }),
        "new" => {
            let (name, slug) = (a.required("name")?, a.text("slug")?);
            render::finish(out, |out| {
                let c = open(sel, env, out)?.create(&name, slug.as_deref())?;
                render::created(out, &c)
            })
        }
        "list" => render::finish(out, |out| {
            let root = open(sel, env, out)?;
            render::list(out, root.path(), &root.list())
        }),
        "lint" => {
            let (rules, changed) = (a.all_named("rule", Rule::from_name), a.flag("changed"));
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::lint(out, &id.slug, &id.lint(&rules, changed))
            })
        }
        "write" => {
            let path = a.required("path")?;
            let content = a.text("content")?.unwrap_or_default();
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::write(out, &path, id.write(&path, &content)?)
            })
        }
        "add" => {
            let kind = a
                .named("kind", Kind::from_type)?
                .ok_or("missing argument \"kind\"")?;
            let title = a.words("title");
            let (date, parent, kb) = (a.day("date")?, a.text("parent")?, a.text("kb")?);
            let aliases = a.strings("alias");
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                let (path, created) = id.add(&owo::AddRequest {
                    kind,
                    title: &title,
                    date,
                    parent: parent.as_deref(),
                    aliases: &aliases,
                    kb: kb.as_deref(),
                })?;
                render::add(out, kind, &path, created)
            })
        }
        "stamp" => {
            let check = a.flag("check");
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::stamp(out, check, &id.stamp(check)?)
            })
        }
        "index" => {
            let check = a.flag("check");
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                let paths = if check { id.stale_index() } else { id.index()? };
                render::index(out, check, &paths)
            })
        }
        "resolve" => {
            let (name, kind) = (a.words("name"), a.named("type", Kind::from_name)?);
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::resolve(out, &name, id.resolve(&name, kind))
            })
        }
        "show" => {
            let (name, section) = (a.words("name"), a.text("section")?);
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                let (path, text) = id.show(&name, section.as_deref())?;
                render::show(out, &path, &text)
            })
        }
        "links" => {
            let name = a.words("name");
            let (inbound, outbound, orphans) = (a.flag("in"), a.flag("out"), a.flag("orphans"));
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                if orphans {
                    return render::orphans(out, &id.orphans());
                }
                render::links(out, &id.links(&name, inbound, outbound)?)
            })
        }
        "context" => {
            let with = a.strings("with");
            let mut req = owo::ContextRequest {
                voice: a.flag("voice"),
                look: a.flag("look"),
                with: &with,
                ..Default::default()
            };
            if let Some(n) = a.int("notes")? {
                req.notes = n;
            }
            // The text, unless the paths are asked for.
            let paths = a.text("format")?.as_deref() == Some("paths");
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                let ctx = id.context(&req)?;
                render::context(out, &id, &ctx, paths)
            })
        }
        "state" => {
            let mut req = owo::StateRequest::default();
            if let Some(days) = a.int("days")? {
                req.days = days;
            }
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::state(out, &id.state(&req)?)
            })
        }
        "search" => {
            let (query, types) = (a.words("query"), a.all_named("type", Kind::from_name));
            let mut req = owo::SearchRequest {
                scope: a.named("scope", Scope::from_name)?,
                types: &types,
                // The project filter is the picked project, as the `--project` flag.
                project: sel.project.as_deref(),
                since: a.day("since")?,
                ..Default::default()
            };
            if let Some(limit) = a.int("limit")? {
                req.limit = limit;
            }
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::search(out, &id.search(&query, &req))
            })
        }
        "review" => render::finish(out, |out| {
            let id = load(sel, env, out)?;
            render::review(out, &id.review())
        }),
        "review_done" => {
            let through = a.day("through")?;
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::review_done(out, &id.review_done(through)?)
            })
        }
        "soul_note" => {
            let (file, change) = (a.required("file")?, a.required("change")?);
            let evidence = a.strings("evidence");
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::soul_note(out, id.soul_note(&file, &change, &evidence)?)
            })
        }
        "soul_history" => {
            let file = a.text("file")?;
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::soul_history(out, &id.soul_history(file.as_deref()))
            })
        }
        "changes" => {
            let (since, days) = (a.text("since")?, a.int("days")?);
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                let req = owo::ChangesRequest {
                    since: since.as_deref(),
                    days,
                };
                render::changes(out, &id.changes(&req)?)
            })
        }
        "export_card" => {
            let scenario = a.text("scenario")?;
            let file = a.text("out")?.map(PathBuf::from);
            let png = a.text("png")?.map(PathBuf::from);
            let mut req = owo::CardRequest {
                // The project filter is the picked project, as the `--project` flag.
                project: sel.project.as_deref(),
                scenario: scenario.as_deref(),
                out: file.as_deref(),
                png: png.as_deref(),
                ..Default::default()
            };
            if let Some(spec) = a.named("spec", Spec::from_name)? {
                req.spec = spec;
            }
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                let card = id.export_card(&req)?;
                render::card(out, card, file.as_deref())
            })
        }
        "export_skill" => {
            let dir = PathBuf::from(a.required("out")?);
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::skill(out, &id.export_skill(&dir)?)
            })
        }
        "export_soulspec" => {
            let dir = PathBuf::from(a.required("out")?);
            render::finish(out, |out| {
                let id = load(sel, env, out)?;
                render::soulspec(out, &dir, id.export_soulspec(&dir)?)
            })
        }
        "sync_start" => {
            let session = a.required("session")?;
            let transcript = PathBuf::from(a.required("transcript")?);
            let format = a
                .named("format", SyncFormat::from_name)?
                .unwrap_or_default();
            let opts = sync_options(a)?;
            render::finish(out, |out| {
                let s = open(sel, env, out)?
                    .sync()?
                    .start(&session, &transcript, format, &opts)?;
                render::synced(out, &s)
            })
        }
        "sync_push" => {
            let session = a.required("session")?;
            let turns = a.get("turns").map(Value::to_string).unwrap_or_default();
            let opts = sync_options(a)?;
            render::finish(out, |out| {
                let root = open(sel, env, out)?;
                let sync = root.sync()?;
                let turns = owo::parse_turns(&turns)?;
                render::synced(out, &sync.push(&session, &turns, &opts)?)
            })
        }
        "sync_done" => {
            let session = a.required("session")?;
            let until = a.int("until")?.unwrap_or_default();
            render::finish(out, |out| {
                let doc = open(sel, env, out)?.sync_done(&session, until)?;
                render::sync_done(out, &session, until, &doc)
            })
        }
        "migrate_v1" => {
            let path = PathBuf::from(a.required("path")?);
            let dest = PathBuf::from(a.required("out")?);
            render::finish(out, |out| {
                render::migrate(out, &owo::migrate_v1(&path, &dest)?)
            })
        }
        _ => return Ok(None),
    };
    Ok(Some(output))
}

/// The `now` and `every` arguments, else the library's defaults.
fn sync_options(a: &Args) -> Result<owo::SyncOptions, String> {
    let mut opts = owo::SyncOptions {
        now: a.flag("now"),
        ..Default::default()
    };
    if let Some(every) = a.int("every")? {
        opts.every = every;
    }
    Ok(opts)
}

fn respond(id: &Value, method: &str, params: &Value, select: &Select, env: &Env) -> Option<Value> {
    let ok = |result: Value| Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    let error = |code: i64, message: String| {
        Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }))
    };
    match method {
        "initialize" => ok(json!({
            "protocolVersion": params["protocolVersion"].as_str().filter(|v| SUPPORTED.contains(v)).unwrap_or(PROTOCOL),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "owo", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "owo manages one identity: soul/, life/ and work/. Start with `context`, read more with `resolve`, `search`, `show` and `links`, create entries with `add`, fill or edit their text with `write`, then run `stamp`, `index` and `lint`.",
        })),
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({
            "tools": tools()
                .iter()
                .map(|t| json!({ "name": t.name, "description": t.about, "inputSchema": schema(t) }))
                .collect::<Vec<_>>()
        })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            match tools().iter().find(|t| t.name == name) {
                Some(tool) => ok(call(tool, params, select, env)),
                None => error(-32602, format!("unknown tool {name:?}")),
            }
        }
        _ => error(-32601, format!("method not found: {method}")),
    }
}

/// The reply to one JSON-RPC message, or None for notifications and responses.
/// Bytes that are not UTF-8 JSON get a parse error.
pub fn reply(message: &[u8], select: &Select, env: &Env) -> Option<Value> {
    match serde_json::from_slice::<Value>(message) {
        Ok(msg) if msg.get("id").is_some() && msg.get("method").is_some() => {
            let method = msg["method"].as_str().unwrap_or_default();
            respond(&msg["id"], method, &msg["params"], select, env)
        }
        Ok(_) => None,
        Err(e) => Some(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": { "code": -32700, "message": format!("parse error: {e}") },
        })),
    }
}

/// Serves MCP on `input` and `output`, one message per line, until `input` ends. `select` gives
/// the default `--root`, `-i` and `--project` of each tool call.
pub fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    select: &Select,
    env: &Env,
) -> Result<()> {
    let mut line = vec![];
    while input.read_until(b'\n', &mut line)? > 0 {
        if !line.trim_ascii().is_empty() {
            if let Some(r) = reply(&line, select, env) {
                writeln!(output, "{r}")?;
                output.flush()?;
            }
        }
        line.clear();
    }
    Ok(())
}

const DEFAULT_PORT: u16 = 4224;
/// Largest request body the HTTP mode reads; a tool call is a few kilobytes.
const MAX_BODY: usize = 8 << 20;
/// Largest request line plus headers the HTTP mode reads.
const MAX_HEAD: usize = 16 << 10;
/// Time one HTTP request may take to arrive, and its reply to leave.
const DEADLINE: Duration = Duration::from_secs(30);

/// The listener for the HTTP mode on `addr`. Without `port` it takes 4224, or the next free
/// port up to 4323. The server URL is `http://<local_addr>/mcp`.
pub fn bind(addr: &str, port: Option<u16>) -> Result<TcpListener> {
    Ok(match port {
        Some(p) => TcpListener::bind((addr, p)).with_context(|| format!("binding {addr}:{p}"))?,
        None => (DEFAULT_PORT..DEFAULT_PORT + 100)
            .find_map(|p| TcpListener::bind((addr, p)).ok())
            .with_context(|| format!("no free port on {addr} from {DEFAULT_PORT}"))?,
    })
}

/// Serves MCP over HTTP on `listener` at `/mcp` (Streamable HTTP with JSON replies), one
/// request at a time.
pub fn serve_http(listener: TcpListener, select: &Select, env: &Env) -> Result<()> {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // A broken or slow client only ends its own request.
        let _ = http_request(stream, select, env);
    }
    Ok(())
}

/// Reads from the stream until one deadline for the whole request.
struct Timed {
    stream: TcpStream,
    until: Instant,
}

impl Read for Timed {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self
            .until
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or(std::io::ErrorKind::TimedOut)?;
        self.stream.set_read_timeout(Some(left))?;
        self.stream.read(buf)
    }
}

fn http_request(stream: TcpStream, select: &Select, env: &Env) -> std::io::Result<()> {
    stream.set_write_timeout(Some(DEADLINE))?;
    let timed = Timed {
        stream: stream.try_clone()?,
        until: Instant::now() + DEADLINE,
    };
    let mut reader = BufReader::new(timed.take((MAX_HEAD + MAX_BODY) as u64));
    let mut request = String::new();
    let mut head = reader.read_line(&mut request)?;
    let (mut length, mut chunked, mut json_body, mut origin) = (None, false, false, false);
    loop {
        let mut header = String::new();
        let n = reader.read_line(&mut header)?;
        head += n;
        if n == 0 || header.trim().is_empty() || head > MAX_HEAD {
            break;
        }
        let Some((k, v)) = header.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        match k.as_str() {
            "content-length" => length = v.parse::<usize>().ok(),
            // owo reads only a Content-Length body, so a chunked one gets 411.
            "transfer-encoding" => chunked = true,
            "content-type" => {
                json_body = v
                    .split(';')
                    .next()
                    .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
            }
            "origin" => origin = true,
            _ => {}
        }
    }
    // Read the body even for a refused request: closing with unread data resets the connection
    // and the client never sees the status.
    let mut body = vec![];
    let length = length.filter(|_| !chunked);
    let mut body_read = false;
    if let Some(length) = length.filter(|n| head <= MAX_HEAD && *n <= MAX_BODY) {
        body.resize(length, 0);
        reader.read_exact(&mut body)?;
        body_read = true;
    }
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let path = path.split('?').next().unwrap_or("");
    let (status, reply) = match (method, path) {
        _ if head > MAX_HEAD => ("431 Request Header Fields Too Large", String::new()),
        // A browser sends Origin with every POST from a page, also after DNS rebinding.
        _ if origin => ("403 Forbidden", String::new()),
        ("POST", "/mcp") if !json_body => ("415 Unsupported Media Type", String::new()),
        ("POST", "/mcp") if length.is_none() => ("411 Length Required", String::new()),
        ("POST", "/mcp") if length > Some(MAX_BODY) => ("413 Payload Too Large", String::new()),
        ("POST", "/mcp") => match reply(&body, select, env) {
            Some(reply) => ("200 OK", reply.to_string()),
            None => ("202 Accepted", String::new()),
        },
        (_, "/mcp") => ("405 Method Not Allowed", String::new()),
        _ => ("404 Not Found", String::new()),
    };
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    )?;
    stream.flush()?;
    // A refused body that was not read (no length, or past a cap) would reset the connection on
    // close and cut the reply. Send the reply's end first, then discard what the client still
    // sends: stop after 200ms with nothing arriving, or after 2 seconds, so an idle client
    // cannot hold this one-at-a-time server.
    if !body_read {
        stream.shutdown(std::net::Shutdown::Write)?;
        stream.set_read_timeout(Some(Duration::from_millis(200)))?;
        let stop = Instant::now() + Duration::from_secs(2);
        let mut buf = [0u8; 8192];
        while Instant::now() < stop && matches!(stream.read(&mut buf), Ok(n) if n > 0) {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_an_error_result_and_the_next_call_runs() {
        let failed = guarded(|| panic!("boom"));
        assert_eq!(failed["isError"], true);
        assert_eq!(failed["content"][0]["text"], "owo panicked: boom");
        let next = guarded(|| result("ok".into(), false));
        assert_eq!(next["isError"], false);
    }

    /// Each tool maps to a library call: `dispatch` gives no `None` for any tool of the list.
    #[test]
    fn every_tool_has_a_library_call() {
        let root = tempfile::TempDir::new().unwrap();
        let sel = Select {
            root: Some(root.path().to_path_buf()),
            ..Select::default()
        };
        let env = Env {
            cwd: root.path().to_path_buf(),
            ..Env::default()
        };
        let tools = tools();
        assert!(tools.len() > 20);
        for tool in &tools {
            let map: Map<String, Value> = tool
                .params
                .iter()
                .filter(|p| p.required)
                .map(|p| {
                    let value = match (p.values.first(), p.kind) {
                        (Some(v), _) => json!(v),
                        (None, "integer") => json!(1),
                        // A path stays inside the temporary root.
                        (None, _) => json!(root.path().join("x")),
                    };
                    (p.name.clone(), value)
                })
                .collect();
            let args = Args::new(tool, &map).unwrap();
            let called = dispatch(tool, &args, &sel, &env);
            assert!(
                !matches!(called, Ok(None)),
                "{} has no library call",
                tool.name
            );
        }
    }
}
