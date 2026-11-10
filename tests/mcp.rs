//! End-to-end test for `owo mcp`: JSON-RPC over stdin and stdout.

mod common;

use common::{examples, owo};
use serde_json::{json, Value};
use std::path::Path;

fn raw_session(root: &Path, id: &str, input: Vec<u8>) -> Vec<Value> {
    let out = owo(root)
        .args(["-i", id, "mcp"])
        .write_stdin(input)
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn lines(messages: &[Value]) -> Vec<u8> {
    messages
        .iter()
        .map(|m| format!("{m}\n"))
        .collect::<String>()
        .into()
}

fn session(messages: &[Value]) -> Vec<Value> {
    raw_session(&examples(), "nicole", lines(messages))
}

fn call(id: u64, name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

fn text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"].as_str().unwrap()
}

#[test]
fn mcp_lists_and_calls_tools() {
    let replies = session(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        call(3, "resolve", json!({ "name": "First Firing" })),
        call(
            4,
            "show",
            json!({ "name": "First Firing", "section": "Lines" }),
        ),
        call(
            5,
            "search",
            json!({ "query": "-i reviewer kiln", "limit": 1 }),
        ),
        call(6, "resolve", json!({ "nope": 1 })),
        call(7, "context", json!({ "voice": true, "no_touch": true })),
        json!({ "jsonrpc": "2.0", "id": 8, "method": "bogus" }),
    ]);
    assert_eq!(replies.len(), 8, "the notification gets no reply");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "owo");
    let tools: Vec<&str> = replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for t in [
        "resolve",
        "search",
        "links",
        "show",
        "context",
        "state",
        "add",
        "stamp",
        "index",
        "lint",
        "new",
        "list",
        "recent",
        "changes",
        "review",
        "review_done",
        "soul_note",
        "soul_history",
        "export_card",
        "export_soulspec",
        "export_skill",
        "sync_start",
        "sync_done",
        "migrate_v1",
        "templates_list",
        "templates_show",
        "slug",
        "config",
    ] {
        assert!(tools.contains(&t), "{t}");
    }
    let resolved: Value = serde_json::from_str(text(&replies[2])).unwrap();
    assert_eq!(resolved["data"]["path"], "life/events/first-firing.md");
    assert!(text(&replies[3]).starts_with("## Lines"));
    assert!(
        text(&replies[4]).contains("life/log/2026-11-02.md"),
        "values never parse as flags"
    );
    assert!(text(&replies[5]).contains("unknown argument"));
    assert!(text(&replies[6]).contains("soul/VOICE.md"));
    assert_eq!(replies[7]["error"]["code"], -32601);
}

#[test]
fn mcp_sends_turns_on_stdin_and_joins_flag_values() {
    let tmp = tempfile::TempDir::new().unwrap();
    owo(tmp.path()).args(["new", "Tess"]).assert().success();
    let turns: Vec<Value> = (1..=200)
        .map(|n| json!({ "id": n, "role": "user", "text": "x".repeat(1000) }))
        .collect();
    let mut input = lines(&[
        call(
            1,
            "sync_push",
            json!({ "session": "m1", "turns": turns, "now": true }),
        ),
        call(
            2,
            "soul_note",
            json!({ "file": "SOUL.md", "change": "-ly adverbs removed" }),
        ),
    ]);
    input.extend(b"\xff\n");
    input.extend(lines(&[
        json!({ "jsonrpc": "2.0", "id": 3, "method": "ping" }),
    ]));
    let replies = raw_session(tmp.path(), "tess", input);
    assert_eq!(replies.len(), 4, "the server survives invalid UTF-8");
    let push: Value = serde_json::from_str(text(&replies[0])).unwrap();
    assert_eq!(
        push["data"]["added"], 200,
        "200KB of turns pass the argument limit"
    );
    assert_eq!(push["data"]["due"], true);
    assert_eq!(
        replies[1]["result"]["isError"],
        false,
        "{}",
        text(&replies[1])
    );
    assert_eq!(replies[2]["error"]["code"], -32700);
    assert!(replies[3]["result"].is_object());
}

#[test]
fn mcp_remote_serves_http() {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_owo"))
        .env_remove("OWO_ROOT")
        .env_remove("OWO_IDENTITY")
        .env_remove("OWO_PROJECT")
        .arg("--root")
        .arg(examples())
        .args(["-i", "nicole", "mcp", "--remote", "--port", "0"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut url)
        .unwrap();
    let host = url
        .trim()
        .strip_prefix("http://")
        .and_then(|u| u.strip_suffix("/mcp"))
        .unwrap()
        .to_string();
    let send = |request: &str, headers: &str, body: &str| {
        let mut s = std::net::TcpStream::connect(&host).unwrap();
        write!(s, "{request} HTTP/1.1\r\n{headers}\r\n{body}").unwrap();
        let mut reply = String::new();
        s.read_to_string(&mut reply).unwrap();
        reply
    };
    let ok = format!("Host: {host}\r\nContent-Type: application/json\r\n");
    let length = |body: &str| format!("Content-Length: {}\r\n", body.len());
    let post = |body: &str| send("POST /mcp", &format!("{ok}{}", length(body)), body);
    let reply = post(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"resolve","arguments":{"name":"First Firing"}}}"#,
    );
    let notify = post(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    let ping = r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#;
    let status = |request: &str, headers: &str| {
        let reply = send(request, &format!("{headers}{}", length(ping)), ping);
        reply.lines().next().unwrap_or("").to_string()
    };
    let port = host.rsplit_once(':').unwrap().1;
    let too_large = send(
        "POST /mcp",
        &format!("{ok}Content-Length: 99999999999\r\n"),
        "",
    );
    let statuses = [
        status("GET /mcp", &ok),
        status("POST /other", &ok),
        status("POST /mcp", &format!("{ok}Origin: http://example.com\r\n")),
        status(
            "POST /mcp",
            // A tunnel forwards its public hostname.
            &format!("Host: example.com:{port}\r\nContent-Type: application/json\r\n"),
        ),
        status("POST /mcp", &format!("Host: localhost:{port}\r\n")),
        status(
            "POST /mcp",
            &format!("Host: localhost:{port}\r\nContent-Type: application/json; charset=utf-8\r\n"),
        ),
    ];
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(reply.starts_with("HTTP/1.1 200 OK"), "{reply}");
    assert!(reply.contains("life/events/first-firing.md"));
    assert!(notify.starts_with("HTTP/1.1 202"), "{notify}");
    assert!(too_large.starts_with("HTTP/1.1 413"), "{too_large}");
    assert_eq!(
        statuses,
        [
            "HTTP/1.1 405 Method Not Allowed",
            "HTTP/1.1 404 Not Found",
            "HTTP/1.1 403 Forbidden",
            "HTTP/1.1 200 OK",
            "HTTP/1.1 415 Unsupported Media Type",
            "HTTP/1.1 200 OK",
        ]
    );
}
