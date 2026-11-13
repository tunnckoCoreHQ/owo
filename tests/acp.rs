//! End-to-end tests for `owo acp`: a fake agent (`tests/fixtures/acp-agent.sh`) behind the proxy.

mod common;

use common::acp::*;
use common::{copy_example, examples, owo};
use serde_json::json;
use tempfile::TempDir;

fn owo_server() -> serde_json::Value {
    json!({ "name": "owo", "command": "owo", "args": ["mcp", "-i", "nicole"], "env": [] })
}

#[test]
fn passes_lines_through_and_stores_turns_once() {
    let tmp = copy_example("nicole");
    let mut p = start(tmp.path(), &[], 7);
    let first = s(
        json!({ "jsonrpc": "2.0", "id": 2, "method": "session/prompt", "params": { "sessionId": "s1", "prompt": [
        { "type": "text", "text": "hello" },
        { "type": "resource_link", "uri": "file:///a.md", "name": "a.md" },
        { "type": "text", "text": "world" },
    ] } }),
    );
    let sent = [
        init(),
        request(
            1,
            "session/new",
            json!({ "cwd": "/home/u", "mcpServers": [] }),
        ),
        r#"{ "jsonrpc": "2.0", "id": 9, "method": "_vendor/ask", "params": {"b": [1, 2]} }"#.into(),
        r#"{"jsonrpc":"2.0","method":"_vendor/note"}"#.into(),
        "not json from the client".into(),
        first.clone(),
        prompt(3, "s1", &["again"]),
        request(
            4,
            "session/load",
            json!({ "sessionId": "s1", "cwd": "/home/u", "mcpServers": [] }),
        ),
    ];
    let replies = [
        vec![
            init_reply(json!({})),
            r#"{"jsonrpc":"2.0","method":"_vendor/hello","params":{"x":1}}"#.into(),
            "not json from the agent".into(),
        ],
        vec![response(1, json!({ "sessionId": "s1" }))],
        vec![r#"{"jsonrpc":"2.0","id":9,"error":{"code":-32601,"message":"no"}}"#.into()],
        vec![],
        vec![],
        vec![
            chunk("s1", Some("m1"), "Hel"),
            update("s1", "agent_thought_chunk", Some("t1"), "thinking"),
            chunk("s1", Some("m1"), "lo"),
            s(
                json!({ "jsonrpc": "2.0", "method": "session/update", "params": { "sessionId": "s1", "update": { "sessionUpdate": "tool_call", "toolCallId": "c1", "title": "Read", "status": "pending" } } }),
            ),
            chunk("s1", Some("m2"), "Bye"),
            end(2),
        ],
        vec![chunk("s1", Some("m3"), "ok"), end(3)],
        vec![
            update("s1", "user_message_chunk", Some("u1"), "hello"),
            chunk("s1", Some("m1"), "Hello"),
            chunk("s1", Some("m2"), "Bye"),
            chunk("s1", None, "replayed with no id"),
            response(4, json!({})),
        ],
    ];
    for (line, reply) in sent.iter().zip(&replies) {
        p.send(line, reply);
    }
    let (code, got) = p.finish();
    assert_eq!(code, 7, "the agent's exit code");
    assert_eq!(got.len(), sent.len());
    for (i, (got, sent)) in got.iter().zip(&sent).enumerate() {
        if i == 5 {
            // Without the systemPrompt capability, the first prompt gets the identity block first.
            let mut got = parse(got);
            let block = got["params"]["prompt"].as_array_mut().unwrap().remove(0);
            assert_eq!(block["type"], "text");
            assert!(block["text"].as_str().unwrap().contains("\"nicole\""));
            assert_eq!(got, parse(sent));
        } else if i == 1 || i == 7 {
            // A session request gets the owo MCP server.
            let mut want = parse(sent);
            want["params"]["mcpServers"] = json!([owo_server()]);
            assert_eq!(parse(got), want);
        } else {
            assert_eq!(got, sent, "line {i} passes byte for byte");
        }
    }
    assert_eq!(
        stored(tmp.path(), "s1"),
        [
            turn("m1.user", "user", "hello\nworld"),
            turn("m1", "assistant", "Hello"),
            turn("m2", "assistant", "Bye"),
            turn("m3.user", "user", "again"),
            turn("m3", "assistant", "ok"),
        ],
        "the load replay adds nothing"
    );
}

#[test]
fn appends_the_system_prompt_and_the_mcp_server() {
    let tmp = copy_example("nicole");
    let mut p = start(tmp.path(), &[], 0);
    let fs_server = json!({ "name": "fs", "command": "/bin/fs", "args": [], "env": [] });
    let sent = [
        init(),
        request(
            1,
            "session/new",
            json!({ "cwd": "/w", "mcpServers": [fs_server] }),
        ),
        request(
            2,
            "session/load",
            json!({ "sessionId": "s2", "cwd": "/w", "mcpServers": [], "systemPrompt": { "mode": "append", "content": [{ "type": "text", "text": "Be brief." }] } }),
        ),
        request(
            3,
            "session/resume",
            json!({ "sessionId": "s2", "cwd": "/w" }),
        ),
        prompt(4, "s2", &["hi"]),
    ];
    let replies = [
        vec![init_reply(json!({ "systemPrompt": {} }))],
        vec![response(1, json!({ "sessionId": "s2" }))],
        vec![response(2, json!({}))],
        vec![response(3, json!({}))],
        vec![chunk("s2", Some("m1"), "yo"), end(4)],
    ];
    for (line, reply) in sent.iter().zip(&replies) {
        p.send(line, reply);
    }
    let (code, got) = p.finish();
    assert_eq!(code, 0);
    assert_eq!(got[0], sent[0]);
    assert_eq!(got[4], sent[4], "a prompt passes unchanged");
    let text = parse(&got[1])["params"]["systemPrompt"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("\"nicole\"") && text.contains("`context`"));
    let block = json!({ "type": "text", "text": text });
    let owo_server = owo_server();
    let mut new = parse(&sent[1]);
    new["params"]["systemPrompt"] = json!({ "mode": "append", "content": [block] });
    new["params"]["mcpServers"] = json!([fs_server, owo_server]);
    assert_eq!(parse(&got[1]), new);
    let mut load = parse(&sent[2]);
    load["params"]["systemPrompt"]["content"] =
        json!([{ "type": "text", "text": "Be brief." }, block]);
    load["params"]["mcpServers"] = json!([owo_server]);
    assert_eq!(parse(&got[2]), load);
    let mut resume = parse(&sent[3]);
    resume["params"]["systemPrompt"] = json!({ "mode": "append", "content": [block] });
    resume["params"]["mcpServers"] = json!([owo_server]);
    assert_eq!(parse(&got[3]), resume);
    assert_eq!(
        stored(tmp.path(), "s2"),
        [turn("m1.user", "user", "hi"), turn("m1", "assistant", "yo")]
    );
}

#[test]
fn without_an_identity_it_only_passes_lines_through() {
    let tmp = TempDir::new().unwrap();
    let mut p = start(tmp.path(), &[], 0);
    let sent = [
        init(),
        request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] })),
        prompt(2, "s3", &["hi"]),
    ];
    let replies = [
        vec![init_reply(json!({ "systemPrompt": {} }))],
        vec![response(1, json!({ "sessionId": "s3" }))],
        vec![chunk("s3", Some("m1"), "yo"), end(2)],
    ];
    for (line, reply) in sent.iter().zip(&replies) {
        p.send(line, reply);
    }
    let (code, got) = p.finish();
    assert_eq!(code, 0);
    assert_eq!(got, sent);
    assert!(!tmp.path().join(".owo").exists(), "nothing is stored");
    owo(tmp.path())
        .args(["acp", "--inject", "--", "true"])
        .assert()
        .code(2);
}

#[test]
fn mcp_does_not_list_acp() {
    let out = owo(&examples())
        .args(["-i", "nicole", "mcp"])
        .write_stdin(format!(
            "{}\n",
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })
        ))
        .output()
        .unwrap();
    let reply = parse(String::from_utf8(out.stdout).unwrap().trim());
    let tools = reply["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "sync_push"));
    assert!(!tools.iter().any(|t| t["name"] == "acp"));
}

#[test]
fn inject_puts_the_identity_context_in_the_prompt() {
    let tmp = copy_example("nicole");
    let context = owo(tmp.path())
        .args(["-i", "nicole", "context"])
        .output()
        .unwrap();
    let context = String::from_utf8(context.stdout).unwrap();
    assert!(!context.trim().is_empty());
    let new = request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] }));

    // With the systemPrompt capability, the context is the system prompt.
    let mut p = start(tmp.path(), &["--inject"], 0);
    p.send(&init(), &[init_reply(json!({ "systemPrompt": {} }))]);
    p.send(&new, &[response(1, json!({ "sessionId": "s1" }))]);
    let (_, got) = p.finish();
    let mut want = parse(&new);
    want["params"]["systemPrompt"] =
        json!({ "mode": "append", "content": [{ "type": "text", "text": context }] });
    assert_eq!(parse(&got[1]), want, "the context, and no owo MCP server");

    // Without it, the context is the first block of the first prompt, and it is not stored.
    let mut p = start(tmp.path(), &["--inject"], 0);
    p.send(&init(), &[init_reply(json!({}))]);
    p.send(&new, &[response(1, json!({ "sessionId": "s2" }))]);
    p.send(
        &prompt(2, "s2", &["hi"]),
        &[chunk("s2", Some("m1"), "yo"), end(2)],
    );
    let (_, got) = p.finish();
    assert_eq!(got[1], new, "the session request passes unchanged");
    let blocks = parse(&got[2])["params"]["prompt"].clone();
    assert_eq!(
        blocks,
        json!([{ "type": "text", "text": context }, { "type": "text", "text": "hi" }])
    );
    assert_eq!(
        stored(tmp.path(), "s2"),
        [turn("m1.user", "user", "hi"), turn("m1", "assistant", "yo")]
    );
}
