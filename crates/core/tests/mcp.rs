//! The MCP server on in-memory streams, with no process.

use owo_core::mcp;
use owo_core::{Env, Select};
use serde_json::{json, Value};

use std::path::Path;
use tempfile::TempDir;

/// A temporary root holding a copy of each example identity in `ids`.
fn root(ids: &[&str]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for id in ids {
        let from = examples.join(id);
        for entry in walkdir::WalkDir::new(&from) {
            let entry = entry.unwrap();
            let target = tmp
                .path()
                .join(id)
                .join(entry.path().strip_prefix(&from).unwrap());
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target).unwrap();
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    tmp
}

/// The environment of a process in `root` with no owo variables.
fn env(root: &Path) -> Env {
    Env {
        cwd: root.to_path_buf(),
        ..Env::default()
    }
}

/// The server's own root, with no identity or project.
fn select(root: &Path) -> Select {
    Select {
        root: Some(root.to_path_buf()),
        ..Select::default()
    }
}

fn reply(message: Value, sel: &Select, env: &Env) -> Value {
    mcp::reply(message.to_string().as_bytes(), sel, env).unwrap()
}
fn call(name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

#[test]
fn mcp_reply_answers_one_message() {
    let tmp = root(&["nicole"]);
    let (sel, env) = (select(tmp.path()), env(tmp.path()));

    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } });
    let r = reply(init, &sel, &env);
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(r["result"]["serverInfo"]["name"], "owo");

    let r = reply(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        &sel,
        &env,
    );
    let names: Vec<&str> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"slug") && names.contains(&"review_done"));
    assert!(!names.contains(&"mcp") && !names.contains(&"acp"));

    let r = reply(call("slug", json!({ "text": "Hello World" })), &sel, &env);
    assert_eq!(r["result"]["isError"], false);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    let envelope: Value = serde_json::from_str(text).unwrap();
    assert_eq!(envelope["data"]["slug"], "hello-world");

    let r = reply(call("list", json!({})), &sel, &env);
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    let envelope: Value = serde_json::from_str(text).unwrap();
    assert_eq!(envelope["data"]["identities"][0]["slug"], "nicole");

    let r = reply(call("context", json!({ "identity": "nicole" })), &sel, &env);
    assert_eq!(r["result"]["isError"], false);
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Nicole"));

    let r = reply(call("context", json!({ "identity": "nobody" })), &sel, &env);
    assert_eq!(r["result"]["isError"], true);

    assert!(mcp::reply(
        b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}",
        &sel,
        &env
    )
    .is_none());
    assert_eq!(
        mcp::reply(b"not json", &sel, &env).unwrap()["error"]["code"],
        -32700
    );
}

#[test]
fn mcp_serves_a_reader_and_a_writer() {
    let tmp = root(&["nicole"]);
    let (sel, env) = (select(tmp.path()), env(tmp.path()));
    let input = format!(
        "{}\n\n{}\n{}\n",
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        call("slug", json!({ "text": "a b" })),
    );
    let mut output = vec![];
    mcp::serve(input.as_bytes(), &mut output, &sel, &env).unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], json!({ "jsonrpc": "2.0", "id": 1, "result": {} }));
    assert_eq!(lines[1]["result"]["isError"], false);
}

#[test]
fn mcp_dry_run_names_the_file_and_writes_none() {
    let tmp = root(&["nicole"]);
    let (sel, env) = (select(tmp.path()), env(tmp.path()));
    let args =
        json!({ "identity": "nicole", "kind": "character", "title": "Mara Voss", "dry-run": true });
    let r = reply(call("add", args), &sel, &env);
    assert_eq!(r["result"]["isError"], false, "{r}");
    let text = r["result"]["content"][0]["text"].as_str().unwrap();
    let envelope: Value = serde_json::from_str(text).unwrap();
    let file = tmp.path().join("nicole/life/characters/mara-voss.md");
    assert_eq!(
        envelope["data"]["would_write"],
        json!([file.display().to_string()])
    );
    assert!(!file.exists());
}
