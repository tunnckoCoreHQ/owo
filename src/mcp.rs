//! `owo mcp`: a stdio MCP server. Each tool runs this same binary with the matching
//! command line, so a tool call and a CLI call share one code path and one output.
//! Messages are JSON-RPC 2.0: one per line on stdio, or one per POST to `/mcp` with `--remote`.

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PROTOCOL: &str = "2025-06-18";

enum Arg {
    /// A value after `--`.
    Positional,
    /// `--flag <value>`, repeated for arrays.
    Flag(String),
    /// `--flag` when true.
    Switch(String),
    /// A JSON value written to the child's stdin.
    Stdin,
}

struct Param {
    name: String,
    /// JSON schema type: string, integer, boolean, or array (of strings; of objects for stdin).
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
        &<crate::cli::Cli as clap::CommandFactory>::command(),
        &[],
        &mut out,
    );
    out
}

fn tool(cmd: &clap::Command, path: &[&str]) -> Tool {
    let name = path.join("_").replace('-', "_");
    let command: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    let params = cmd
        .get_arguments()
        .filter(|a| !a.is_global_set() && !a.is_hide_set())
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
    // `sync push --turns` takes a JSON array, which can pass the argument size limit,
    // so the tool takes a real array and sends it on stdin.
    let stdin = a.get_id() == "turns";
    let kind = if switch {
        "boolean"
    } else if a.is_positional() {
        // A multi-word positional (a title, a query) is one string.
        "string"
    } else if stdin || matches!(a.get_action(), clap::ArgAction::Append) {
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
            _ if stdin => Arg::Stdin,
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
            let item = if matches!(prm.arg, Arg::Stdin) {
                "object"
            } else {
                "string"
            };
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

/// The command line and stdin for a tool call. Positionals go after `--` and flag values
/// are joined as `--flag=value`, so values never parse as flags.
fn argv(
    tool: &Tool,
    args: &Map<String, Value>,
    cli: &crate::cli::Cli,
) -> Result<(Vec<String>, String), String> {
    if let Some(k) = args
        .keys()
        .find(|k| *k != "identity" && *k != "project" && !tool.params.iter().any(|p| p.name == **k))
    {
        return Err(format!("unknown argument {k:?} for {}", tool.name));
    }
    let mut out: Vec<String> = vec![];
    if let Some(root) = &cli.root {
        out.extend(["--root".into(), root.display().to_string()]);
    }
    let pick = |key: &str, default: &Option<String>| {
        args.get(key).and_then(scalar).or_else(|| default.clone())
    };
    if let Some(i) = pick("identity", &cli.identity) {
        out.extend(["-i".into(), i]);
    }
    if let Some(p) = pick("project", &cli.project) {
        out.extend(["--project".into(), p]);
    }
    if tool.json {
        out.push("--json".into());
    }
    out.extend(tool.command.iter().map(|s| s.to_string()));
    let mut positionals = vec![];
    let mut stdin = String::new();
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
            Arg::Flag(f) => out.extend(values.into_iter().map(|value| format!("{f}={value}"))),
            Arg::Switch(f) => {
                if v.as_bool() == Some(true) {
                    out.push(f.clone());
                }
            }
            Arg::Stdin => stdin = v.to_string(),
        }
    }
    if !positionals.is_empty() {
        out.push("--".into());
        out.extend(positionals);
    }
    Ok((out, stdin))
}

fn call(params: &Value, cli: &crate::cli::Cli) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let empty = Map::new();
    let args = params["arguments"].as_object().unwrap_or(&empty);
    let result = |text: String, error: bool| json!({ "content": [{ "type": "text", "text": text }], "isError": error });
    let tools = tools();
    let Some(tool) = tools.iter().find(|t| t.name == name) else {
        return result(format!("unknown tool {name:?}"), true);
    };
    let (argv, stdin) = match argv(tool, args, cli) {
        Ok(a) => a,
        Err(e) => return result(e, true),
    };
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("owo"));
    let run = || {
        let mut child = Command::new(exe)
            .args(&argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // A child that exits before reading (a usage error) closes the pipe; its output says why.
        let _ = child
            .stdin
            .take()
            .expect("piped")
            .write_all(stdin.as_bytes());
        child.wait_with_output()
    };
    match run() {
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

fn respond(id: &Value, method: &str, params: &Value, cli: &crate::cli::Cli) -> Option<Value> {
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
        "tools/call" => ok(call(params, cli)),
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("method not found: {method}") },
        })),
    }
}

/// The reply to one JSON-RPC message, or None for notifications and responses.
/// Bytes that are not UTF-8 JSON get a parse error.
fn handle(bytes: &[u8], cli: &crate::cli::Cli) -> Option<Value> {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(msg) if msg.get("id").is_some() && msg.get("method").is_some() => {
            let method = msg["method"].as_str().unwrap_or_default();
            respond(&msg["id"], method, &msg["params"], cli)
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
pub fn serve(cli: &crate::cli::Cli) -> Result<()> {
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout();
    let mut line = vec![];
    while stdin.read_until(b'\n', &mut line)? > 0 {
        if !line.trim_ascii().is_empty() {
            if let Some(r) = handle(&line, cli) {
                writeln!(stdout, "{r}")?;
                stdout.flush()?;
            }
        }
        line.clear();
    }
    Ok(())
}

pub const DEFAULT_PORT: u16 = 4224;
/// Largest request body the HTTP mode reads; a tool call is a few kilobytes.
pub const MAX_BODY: usize = 8 << 20;
/// Largest request line plus headers the HTTP mode reads.
const MAX_HEAD: usize = 16 << 10;
/// Time one HTTP request may take to arrive, and its reply to leave.
const DEADLINE: Duration = Duration::from_secs(30);

/// Serves MCP over HTTP at `http://<addr>:<port>/mcp` (Streamable HTTP with JSON replies).
/// Without `port` it takes 4224, or the next free port up to 4323. Prints the URL, then
/// handles one request at a time.
pub fn serve_http(addr: &str, port: Option<u16>, cli: &crate::cli::Cli) -> Result<()> {
    let listener = match port {
        Some(p) => TcpListener::bind((addr, p)).with_context(|| format!("binding {addr}:{p}"))?,
        None => (DEFAULT_PORT..DEFAULT_PORT + 100)
            .find_map(|p| TcpListener::bind((addr, p)).ok())
            .with_context(|| format!("no free port on {addr} from {DEFAULT_PORT}"))?,
    };
    let bound = listener.local_addr()?;
    println!("http://{bound}/mcp");
    std::io::stdout().flush()?;
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // A broken or slow client only ends its own request.
        let _ = http_request(stream, cli);
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

fn http_request(stream: TcpStream, cli: &crate::cli::Cli) -> std::io::Result<()> {
    stream.set_write_timeout(Some(DEADLINE))?;
    let timed = Timed {
        stream: stream.try_clone()?,
        until: Instant::now() + DEADLINE,
    };
    let mut reader = BufReader::new(timed.take((MAX_HEAD + MAX_BODY) as u64));
    let mut request = String::new();
    let mut head = reader.read_line(&mut request)?;
    let (mut length, mut json_body, mut origin) = (0, false, false);
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
            "content-length" => length = v.parse().unwrap_or(0),
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
    if head <= MAX_HEAD && length <= MAX_BODY {
        body.resize(length, 0);
        reader.read_exact(&mut body)?;
    }
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let path = path.split('?').next().unwrap_or("");
    let (status, reply) = match (method, path) {
        _ if head > MAX_HEAD => ("431 Request Header Fields Too Large", String::new()),
        // A browser sends Origin with every POST from a page, also after DNS rebinding.
        _ if origin => ("403 Forbidden", String::new()),
        ("POST", "/mcp") if !json_body => ("415 Unsupported Media Type", String::new()),
        ("POST", "/mcp") if length > MAX_BODY => ("413 Payload Too Large", String::new()),
        ("POST", "/mcp") => match handle(&body, cli) {
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
    stream.flush()
}
