//! End-to-end test for `owo mcp`: JSON-RPC over stdin and stdout.

use assert_cmd::Command;
use serde_json::{json, Value};
use std::path::PathBuf;

fn session(messages: &[Value]) -> Vec<Value> {
    let input: String = messages.iter().map(|m| format!("{m}\n")).collect();
    let out = Command::cargo_bin("owo")
        .unwrap()
        .env_remove("OWO_ROOT")
        .env_remove("OWO_IDENTITY")
        .env_remove("OWO_PROJECT")
        .arg("--root")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples"))
        .args(["-i", "nicole", "mcp"])
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
fn mcp_remote_serves_http() {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_owo"))
        .env_remove("OWO_ROOT")
        .env_remove("OWO_IDENTITY")
        .env_remove("OWO_PROJECT")
        .arg("--root")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples"))
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
    let post = |body: &str| {
        let mut s = std::net::TcpStream::connect(&host).unwrap();
        write!(
            s,
            "POST /mcp HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut reply = String::new();
        s.read_to_string(&mut reply).unwrap();
        reply
    };
    let reply = post(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"resolve","arguments":{"name":"First Firing"}}}"#,
    );
    let notify = post(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(reply.starts_with("HTTP/1.1 200 OK"), "{reply}");
    assert!(reply.contains("life/events/first-firing.md"));
    assert!(notify.starts_with("HTTP/1.1 202"), "{notify}");
}
