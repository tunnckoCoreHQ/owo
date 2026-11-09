//! `owo mcp`: a stdio MCP server. Each tool runs this same binary with the matching
//! command line, so a tool call and a CLI call share one code path and one output.
//! Messages are JSON-RPC 2.0: one per line on stdio, or one per POST to `/mcp` with `--remote`.

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const PROTOCOL: &str = "2025-06-18";

enum Arg {
    /// A value after `--`.
    Positional,
    /// `--flag <value>`, repeated for arrays.
    Flag(String),
    /// `--flag` when true.
    Switch(String),
}

struct Param {
    name: String,
    /// JSON schema type: string, integer, boolean, or array (of strings).
    kind: &'static str,
    arg: Arg,
    required: bool,
    about: String,
    /// Allowed values, when the flag has a fixed list.
    values: Vec<String>,
}

struct Tool {
    name: String,
    about: String,
    /// Subcommand and fixed flags.
    command: Vec<String>,
    /// Run with `--json` and return the envelope; otherwise return the text output.
    json: bool,
    params: Vec<Param>,
}

/// Every command as a tool, read from the clap definitions: `review done` becomes `review_done`.
/// A command with optional subcommands (`review`) is a tool itself too. `mcp` is left out.
fn tools() -> Vec<Tool> {
    fn walk(cmd: &clap::Command, path: &[&str], out: &mut Vec<Tool>) {
        for sub in cmd
            .get_subcommands()
            .filter(|s| !matches!(s.get_name(), "mcp" | "help"))
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
        &<crate::Cli as clap::CommandFactory>::command(),
        &[],
        &mut out,
    );
    out
}

fn tool(cmd: &clap::Command, path: &[&str]) -> Tool {
    let name = path.join("_").replace('-', "_");
    let mut command: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    // An agent reads context as text, so the format is fixed.
    if name == "context" {
        command.extend(["--format".into(), "text".into()]);
    }
    let params = cmd
        .get_arguments()
        .filter(|a| !a.is_global_set() && !a.is_hide_set())
        .filter(|a| !(name == "context" && a.get_id() == "format"))
        .map(param)
        .collect();
    Tool {
        json: !matches!(name.as_str(), "context" | "show"),
        about: cmd.get_about().map(|a| a.to_string()).unwrap_or_default(),
        name,
        command,
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
    let kind = if switch {
        "boolean"
    } else if a.is_positional() {
        // A multi-word positional (a title, a query) is one string.
        "string"
    } else if matches!(a.get_action(), clap::ArgAction::Append) {
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
        arg: match (a.is_positional(), switch) {
            (true, _) => Arg::Positional,
            (false, true) => Arg::Switch(format!("--{long}")),
            (false, false) => Arg::Flag(format!("--{long}")),
        },
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
    for prm in &tool.params {
        let mut s = json!({ "type": prm.kind, "description": prm.about });
        let values = (!prm.values.is_empty()).then(|| json!(prm.values));
        if prm.kind == "array" {
            s["items"] = json!({ "type": "string" });
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

/// The command line for a tool call. Positionals go after `--`, so values never parse as flags.
fn argv(tool: &Tool, args: &Map<String, Value>, d: &Defaults) -> Result<Vec<String>, String> {
    if let Some(k) = args
        .keys()
        .find(|k| *k != "identity" && *k != "project" && !tool.params.iter().any(|p| p.name == **k))
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
    for prm in &tool.params {
        let Some(v) = args.get(&prm.name).filter(|v| !v.is_null()) else {
            if prm.required {
                return Err(format!("missing argument {:?}", prm.name));
            }
            continue;
        };
        let values: Vec<String> = match v {
            Value::Array(items) => items.iter().filter_map(scalar).collect(),
            other => scalar(other).into_iter().collect(),
        };
        match &prm.arg {
            Arg::Positional => positionals.extend(values),
            Arg::Flag(f) => {
                for value in values {
                    out.extend([f.clone(), value]);
                }
            }
            Arg::Switch(f) => {
                if v.as_bool() == Some(true) {
                    out.push(f.clone());
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
    let tools = tools();
    let Some(tool) = tools.iter().find(|t| t.name == name) else {
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
            "instructions": "owo manages one identity: soul/, life/ and work/. Start with `context`, read more with `resolve`, `search`, `show` and `links`, create entries with `add`, fill or edit their text with `write`, then run `stamp`, `index` and `lint`.",
        })),
        "ping" => ok(json!({})),
        "tools/list" => ok(json!({
            "tools": tools()
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

/// The reply to one JSON-RPC message, or None for notifications and responses.
fn handle(text: &str, d: &Defaults) -> Option<Value> {
    match serde_json::from_str::<Value>(text) {
        Ok(msg) if msg.get("id").is_some() && msg.get("method").is_some() => {
            let method = msg["method"].as_str().unwrap_or_default();
            respond(&msg["id"], method, &msg["params"], d)
        }
        Ok(_) => None,
        Err(e) => Some(json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": { "code": -32700, "message": format!("parse error: {e}") },
        })),
    }
}

/// Serves MCP on stdin and stdout until stdin closes.
pub fn serve(d: &Defaults) -> Result<()> {
    let mut stdout = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(r) = handle(&line, d) {
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

pub const DEFAULT_PORT: u16 = 4224;
/// Largest request body the HTTP mode reads; a tool call is a few kilobytes.
pub const MAX_BODY: usize = 8 << 20;

/// Serves MCP over HTTP at `http://<addr>:<port>/mcp` (Streamable HTTP with JSON replies).
/// Without `port` it takes 4224, or the next free port up to 4323. Prints the URL, then
/// handles one request at a time.
pub fn serve_http(addr: &str, port: Option<u16>, d: &Defaults) -> Result<()> {
    let listener = match port {
        Some(p) => TcpListener::bind((addr, p)).with_context(|| format!("binding {addr}:{p}"))?,
        None => (DEFAULT_PORT..DEFAULT_PORT + 100)
            .find_map(|p| TcpListener::bind((addr, p)).ok())
            .with_context(|| format!("no free port on {addr} from {DEFAULT_PORT}"))?,
    };
    println!("http://{}/mcp", listener.local_addr()?);
    std::io::stdout().flush()?;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // A broken or slow client only ends its own request.
        let _ = http_request(stream, d);
    }
    Ok(())
}

fn http_request(stream: TcpStream, d: &Defaults) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = header.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let path = path.split('?').next().unwrap_or("");
    let (status, body) = match (method, path) {
        ("POST", "/mcp") if length > MAX_BODY => ("413 Payload Too Large", String::new()),
        ("POST", "/mcp") => {
            let mut body = vec![0; length];
            reader.read_exact(&mut body)?;
            match handle(&String::from_utf8_lossy(&body), d) {
                Some(reply) => ("200 OK", reply.to_string()),
                None => ("202 Accepted", String::new()),
            }
        }
        (_, "/mcp") => ("405 Method Not Allowed", String::new()),
        _ => ("404 Not Found", String::new()),
    };
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}
