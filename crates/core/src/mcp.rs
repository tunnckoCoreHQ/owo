use crate::request::{scalar, Args, Field, Type};
use crate::{operations, Env, Envelope, Operation, Outcome, Ran, Select};
use anyhow::Context;
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

const PROTOCOL: &str = "2025-06-18";
const SUPPORTED: &[&str] = &[PROTOCOL, "2025-03-26", "2024-11-05"];

fn returns_text(op: &Operation) -> bool {
    matches!(op.path, ["context"] | ["show"] | ["soul"])
}

fn schema_type(f: &Field) -> &'static str {
    match f.ty {
        Type::Switch => "boolean",
        _ if f.positional => "string",
        Type::Json => "array",
        _ if f.many => "array",
        Type::Int => "integer",
        _ => "string",
    }
}

fn schema(op: &Operation) -> Value {
    let mut props = json!({
        "identity": { "type": "string", "description": "Identity slug (default: the server's)" },
        "project": { "type": "string", "description": "Project slug" },
    });
    for f in &op.fields {
        let kind = schema_type(f);
        let mut s = json!({ "type": kind, "description": f.help });
        let at = if kind == "array" {
            s["items"] = json!({ "type": if f.ty == Type::Json { "object" } else { "string" } });
            &mut s["items"]
        } else {
            &mut s
        };
        if let Type::Choice(v) = &f.ty {
            at["enum"] = json!(v);
        }
        props[f.name] = s;
    }
    let required: Vec<&str> = op
        .fields
        .iter()
        .filter(|f| f.required)
        .map(|f| f.name)
        .collect();
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}
fn result(text: String, error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": error })
}

fn call(op: &Operation, params: &Value, select: &Select, env: &Env) -> Value {
    guarded(|| run_tool(op, params, select, env))
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

fn run_tool(op: &Operation, params: &Value, select: &Select, env: &Env) -> Value {
    let empty = Map::new();
    let map = params["arguments"].as_object().unwrap_or(&empty);
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
    let pick = |key: &str, default: &Option<String>| {
        map.get(key).and_then(scalar).or_else(|| default.clone())
    };
    let select = Select {
        root: select.root.clone(),
        identity: pick("identity", &select.identity),
        project: pick("project", &select.project),
    };
    let mut args = map.clone();
    args.remove("identity");
    args.remove("project");
    for f in op.fields.iter().filter(|f| f.ty == Type::Json) {
        if let Some(v) = args.get_mut(f.name) {
            *v = Value::String(v.to_string());
        }
    }
    let ran = Args::new(&op.name(), &op.fields, args)
        .and_then(|args| op.run(&args, &select, env, &mut std::io::empty()));
    match ran {
        Ok(ran) => reply_of(op, ran),
        Err(e) => result(e.to_string(), true),
    }
}

fn reply_of(op: &Operation, ran: Ran) -> Value {
    let (mut warnings, outcome) = (ran.warnings, ran.result);
    let lines = |w: &[String]| w.iter().map(|w| crate::warning_line(w)).collect::<String>();
    let (stdout, stderr, error) = match &outcome {
        _ if !returns_text(op) => {
            let envelope = match &outcome {
                Ok(o) => o.envelope(),
                Err(e) => Envelope::fail(&e.to_string()),
            };
            let text = serde_json::to_string_pretty(&envelope).expect("envelope serializes");
            (format!("{text}\n"), lines(&warnings), !envelope.ok)
        }
        Ok(o) => {
            let envelope = o.envelope();
            warnings.extend(envelope.text_warnings());
            (text(o), lines(&warnings), !envelope.ok)
        }
        Err(e) => ("".into(), format!("{}owo: {e}\n", lines(&warnings)), true),
    };
    let text = match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, _) => stderr,
        (false, true) => stdout,
        (false, false) => format!("{stdout}\n{stderr}"),
    };
    result(text, error)
}

fn text(o: &Outcome) -> String {
    match o {
        Outcome::Context { text, .. } | Outcome::Show { text, .. } | Outcome::Soul { text, .. } => {
            text.clone()
        }
        _ => String::new(),
    }
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
            "tools": operations()
                .iter()
                .map(|op| json!({ "name": op.name(), "description": op.about, "inputSchema": schema(op) }))
                .collect::<Vec<_>>()
        })),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            match operations().iter().find(|op| op.name() == name) {
                Some(op) => ok(call(op, params, select, env)),
                None => error(-32602, format!("unknown tool {name:?}")),
            }
        }
        _ => error(-32601, format!("method not found: {method}")),
    }
}

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

pub fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    select: &Select,
    env: &Env,
) -> std::io::Result<()> {
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
const MAX_BODY: usize = 8 << 20;
const MAX_HEAD: usize = 16 << 10;
const DEADLINE: Duration = Duration::from_secs(30);

pub fn bind(addr: &str, port: Option<u16>) -> crate::Result<TcpListener> {
    Ok(match port {
        Some(p) => TcpListener::bind((addr, p)).with_context(|| format!("binding {addr}:{p}"))?,
        None => (DEFAULT_PORT..DEFAULT_PORT + 100)
            .find_map(|p| TcpListener::bind((addr, p)).ok())
            .with_context(|| format!("no free port on {addr} from {DEFAULT_PORT}"))?,
    })
}

pub fn serve_http(listener: TcpListener, select: &Select, env: &Env) -> std::io::Result<()> {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let _ = http_request(stream, select, env);
    }
    Ok(())
}

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
}
