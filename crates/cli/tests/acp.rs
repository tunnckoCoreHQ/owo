mod common;

use common::acp::*;
use common::{copy_example, examples, owo};
use serde_json::json;
use tempfile::TempDir;

fn nicole_only(current: &str) -> serde_json::Value {
    json!([picker(current, &["nicole"])])
}

fn owo_bin() -> String {
    assert_cmd::cargo::cargo_bin("owo").display().to_string()
}

fn owo_server() -> serde_json::Value {
    json!({ "name": "owo", "command": owo_bin(), "args": ["mcp", "-i", "nicole"], "env": [] })
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
    for (i, (line, reply)) in sent.iter().zip(&replies).enumerate() {
        if i == 1 || i == 7 {
            p.open(line, reply, "s1", nicole_only("nicole"));
        } else {
            p.send(line, reply);
        }
    }
    let (code, got) = p.finish();
    assert_eq!(code, 7, "the agent's exit code");
    assert_eq!(got.len(), sent.len());
    for (i, (got, sent)) in got.iter().zip(&sent).enumerate() {
        if i == 5 {
            let mut got = parse(got);
            let block = got["params"]["prompt"].as_array_mut().unwrap().remove(0);
            assert_eq!(block["type"], "text");
            assert!(block["text"].as_str().unwrap().contains("\"nicole\""));
            assert_eq!(got, parse(sent));
        } else if i == 1 || i == 7 {
            let mut want = parse(sent);
            want["params"]["mcpServers"] = json!([owo_server()]);
            assert_eq!(parse(got), want);
        } else {
            assert_eq!(got, sent, "line {i} passes byte for byte");
        }
    }
    assert_eq!(
        stored(tmp.path(), "s1.nicole"),
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
fn adds_the_mcp_server_to_each_session_request() {
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
    for (i, (line, reply)) in sent.iter().zip(&replies).enumerate() {
        match i {
            1..=3 => p.open(line, reply, "s2", nicole_only("nicole")),
            _ => p.send(line, reply),
        }
    }
    let (code, got) = p.finish();
    assert_eq!(code, 0);
    assert_eq!(got[0], sent[0]);
    for (i, servers) in [
        (1, json!([fs_server, owo_server()])),
        (2, json!([owo_server()])),
        (3, json!([owo_server()])),
    ] {
        let mut want = parse(&sent[i]);
        want["params"]["mcpServers"] = servers;
        assert_eq!(parse(&got[i]), want, "only the server list changes");
    }
    let mut first = parse(&got[4]);
    let block = first["params"]["prompt"].as_array_mut().unwrap().remove(0);
    let text = block["text"].as_str().unwrap();
    assert!(text.contains("`context`") && text.contains("argument `identity` set to \"nicole\""));
    assert_eq!(first, parse(&sent[4]));
    assert_eq!(
        stored(tmp.path(), "s2.nicole"),
        [turn("m1.user", "user", "hi"), turn("m1", "assistant", "yo")]
    );
}

#[test]
fn with_an_empty_root_it_only_passes_lines_through() {
    let tmp = TempDir::new().unwrap();
    let mut p = start(tmp.path(), &[], 0);
    let sent = [
        init(),
        request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] })),
        prompt(2, "s3", &["hi"]),
        request(
            3,
            "session/set_config_option",
            json!({ "sessionId": "s3", "configId": "identity", "value": "nicole" }),
        ),
        prompt(4, "s3", &["/owo nicole"]),
    ];
    let replies = [
        vec![init_reply(json!({ "systemPrompt": {} }))],
        vec![response(1, json!({ "sessionId": "s3" }))],
        vec![chunk("s3", Some("m1"), "yo"), end(2)],
        vec![response(3, json!({ "configOptions": [] }))],
        vec![s(commands("s3", &[])), end(4)],
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
fn inject_puts_the_identity_context_in_the_first_prompt() {
    let tmp = copy_example("nicole");
    let context = owo(tmp.path())
        .args(["-i", "nicole", "context"])
        .output()
        .unwrap();
    let context = String::from_utf8(context.stdout).unwrap();
    assert!(!context.trim().is_empty());
    let new = request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] }));
    let mut p = start(tmp.path(), &["--inject"], 0);
    p.send(&init(), &[init_reply(json!({ "systemPrompt": {} }))]);
    p.open(
        &new,
        &[response(1, json!({ "sessionId": "s2" }))],
        "s2",
        nicole_only("nicole"),
    );
    p.send(
        &prompt(2, "s2", &["hi"]),
        &[chunk("s2", Some("m1"), "yo"), end(2)],
    );
    p.send(&prompt(3, "s2", &["more"]), &[end(3)]);
    let (_, got) = p.finish();
    assert_eq!(got[1], new, "no owo MCP server and no other change");
    let blocks = parse(&got[2])["params"]["prompt"].clone();
    assert_eq!(
        blocks,
        json!([{ "type": "text", "text": context }, { "type": "text", "text": "hi" }])
    );
    assert_eq!(got[3], prompt(3, "s2", &["more"]), "only the first prompt");
    assert_eq!(
        stored(tmp.path(), "s2.nicole")[..2],
        [turn("m1.user", "user", "hi"), turn("m1", "assistant", "yo")],
        "the context is not stored"
    );
}

fn mode(current: &str) -> serde_json::Value {
    json!({ "id": "mode", "name": "Mode", "type": "select", "currentValue": current, "options": [{ "value": "ask", "name": "Ask" }, { "value": "code", "name": "Code" }] })
}

fn block(line: &str) -> (String, serde_json::Value) {
    let mut msg = parse(line);
    let block = msg["params"]["prompt"].as_array_mut().unwrap().remove(0);
    (block["text"].as_str().unwrap().to_string(), msg)
}

#[test]
fn the_picker_switches_the_identity_of_a_session() {
    let tmp = both();
    let ids = ["nicole", "reviewer"];
    let mut p = start(tmp.path(), &["-i", "nicole"], 0);
    p.send(&init(), &[init_reply(json!({}))]);
    p.open(
        &request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] })),
        &[response(
            1,
            json!({ "sessionId": "s", "configOptions": [mode("ask")] }),
        )],
        "s",
        json!([mode("ask"), picker("nicole", &ids)]),
    );
    let web = json!({ "name": "web", "description": "Search the web" });
    let agent_commands = session_update(
        "s",
        json!({ "sessionUpdate": "available_commands_update", "availableCommands": [web] }),
    );
    let got = p.call(
        &prompt(2, "s", &["hi"]),
        &[s(agent_commands), chunk("s", Some("m1"), "yo"), end(2)],
        3,
    );
    assert_eq!(parse(&got[0]), commands("s", &[web]));
    assert_eq!(got[1..], [chunk("s", Some("m1"), "yo"), end(2)]);
    let set_mode = request(
        3,
        "session/set_config_option",
        json!({ "sessionId": "s", "configId": "mode", "value": "code" }),
    );
    let agent_update = session_update(
        "s",
        json!({ "sessionUpdate": "config_option_update", "configOptions": [mode("code")] }),
    );
    let got = p.call(
        &set_mode,
        &[
            response(3, json!({ "configOptions": [mode("code")] })),
            s(agent_update.clone()),
        ],
        2,
    );
    let full = json!([mode("code"), picker("nicole", &ids)]);
    assert_eq!(
        parse(&got[0]),
        parse(&response(3, json!({ "configOptions": full })))
    );
    let mut want = agent_update;
    want["params"]["update"]["configOptions"] = full;
    assert_eq!(parse(&got[1]), want);
    let got = p.ask(
        &request(
            4,
            "session/set_config_option",
            json!({ "sessionId": "s", "configId": "identity", "value": "reviewer" }),
        ),
        1,
    );
    let full = json!([mode("code"), picker("reviewer", &ids)]);
    assert_eq!(
        parse(&got[0]),
        parse(&response(4, json!({ "configOptions": full })))
    );
    let got = p.ask(
        &request(
            5,
            "session/set_config_option",
            json!({ "sessionId": "s", "configId": "identity", "value": "nosuch" }),
        ),
        1,
    );
    let error = parse(&got[0]);
    assert_eq!(
        (&error["id"], &error["error"]["code"]),
        (&json!(5), &json!(-32602))
    );
    p.send(
        &prompt(6, "s", &["next"]),
        &[chunk("s", Some("m2"), "ok"), end(6)],
    );
    p.open(
        &request(
            7,
            "session/load",
            json!({ "sessionId": "t", "cwd": "/w", "mcpServers": [] }),
        ),
        &[
            chunk("t", Some("m9"), "old"),
            response(7, json!({ "configOptions": [mode("ask")] })),
        ],
        "t",
        json!([mode("ask"), picker("nicole", &ids)]),
    );
    let (_, got) = p.finish();
    assert_eq!(got.len(), 6, "the identity requests never reach the agent");
    let (text, _) = block(&got[2]);
    assert!(text.contains("\"nicole\""));
    assert_eq!(got[3], set_mode);
    let (text, rest) = block(&got[4]);
    assert!(text.contains("argument `identity` set to \"reviewer\""));
    assert_eq!(rest, parse(&prompt(6, "s", &["next"])));
    assert_eq!(
        stored(tmp.path(), "s.nicole"),
        [turn("m1.user", "user", "hi"), turn("m1", "assistant", "yo")]
    );
    assert_eq!(
        stored(tmp.path(), "s.reviewer"),
        [
            turn("m2.user", "user", "next"),
            turn("m2", "assistant", "ok")
        ],
        "the turns after the switch"
    );
    assert_eq!(
        stored(tmp.path(), "t.nicole"),
        [turn("m9", "assistant", "old")]
    );
}

#[test]
fn the_owo_command_switches_the_identity_of_a_session() {
    let tmp = both();
    let ids = ["nicole", "reviewer"];
    let mut p = start(tmp.path(), &["-i", "nicole"], 0);
    p.send(&init(), &[init_reply(json!({}))]);
    p.open(
        &request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] })),
        &[response(1, json!({ "sessionId": "s" }))],
        "s",
        json!([picker("nicole", &ids)]),
    );
    let message = |line: &str| parse(line)["params"]["update"]["content"]["text"].clone();
    let got = p.ask(&prompt(2, "s", &["/owo reviewer"]), 3);
    assert_eq!(
        message(&got[0]),
        "This session now records into \"reviewer\"."
    );
    assert_eq!(
        parse(&got[1]),
        session_update(
            "s",
            json!({ "sessionUpdate": "config_option_update", "configOptions": [picker("reviewer", &ids)] })
        )
    );
    assert_eq!(got[2], end(2));
    p.send(
        &prompt(3, "s", &["hi"]),
        &[chunk("s", Some("m1"), "yo"), end(3)],
    );
    let got = p.ask(&prompt(4, "s", &["/owo"]), 2);
    assert_eq!(
        message(&got[0]),
        "This session records into \"reviewer\". Switch with /owo <name>: nicole, reviewer, none."
    );
    assert_eq!(got[1], end(4));
    let got = p.ask(&prompt(5, "s", &["/owo nosuch"]), 2);
    assert!(message(&got[0]).as_str().unwrap().starts_with(
        "There is no owo identity \"nosuch\". This session records into \"reviewer\"."
    ));
    assert_eq!(got[1], end(5));
    let got = p.ask(&prompt(6, "s", &["/owo none"]), 3);
    assert_eq!(
        message(&got[0]),
        "This session now records into no identity."
    );
    assert_eq!(
        parse(&got[1])["params"]["update"]["configOptions"],
        json!([picker("_none", &ids)])
    );
    assert_eq!(got[2], end(6));
    p.send(
        &prompt(7, "s", &["quiet"]),
        &[chunk("s", Some("m2"), "sh"), end(7)],
    );
    let (_, got) = p.finish();
    assert_eq!(got.len(), 4, "no command reaches the agent");
    let (text, _) = block(&got[2]);
    assert!(
        text.contains("set to \"reviewer\""),
        "the block of the new identity"
    );
    assert_eq!(got[3], prompt(7, "s", &["quiet"]), "none injects nothing");
    assert_eq!(
        stored(tmp.path(), "s.reviewer"),
        [turn("m1.user", "user", "hi"), turn("m1", "assistant", "yo")],
        "neither the commands nor the turns after none"
    );
    assert!(!tmp
        .path()
        .join("nicole/sessions/s.nicole.turns.jsonl")
        .exists());
}

#[test]
fn with_no_identity_at_start_a_pick_starts_the_session() {
    let tmp = both();
    let ids = ["nicole", "reviewer"];
    let mut p = start(tmp.path(), &[], 0);
    p.send(&init(), &[init_reply(json!({}))]);
    let new = request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] }));
    p.open(
        &new,
        &[response(1, json!({ "sessionId": "s" }))],
        "s",
        json!([picker("_none", &ids)]),
    );
    p.send(
        &prompt(2, "s", &["a"]),
        &[chunk("s", Some("m1"), "x"), end(2)],
    );
    assert!(!tmp.path().join(".owo").exists(), "nothing is stored");
    p.ask(
        &request(
            3,
            "session/set_config_option",
            json!({ "sessionId": "s", "configId": "identity", "value": "nicole" }),
        ),
        1,
    );
    p.send(
        &prompt(4, "s", &["b"]),
        &[chunk("s", Some("m2"), "y"), end(4)],
    );
    let (_, got) = p.finish();
    let mut want = parse(&new);
    want["params"]["mcpServers"] =
        json!([{ "name": "owo", "command": owo_bin(), "args": ["mcp"], "env": [] }]);
    assert_eq!(parse(&got[1]), want, "the server has no identity flag");
    assert_eq!(got[2], prompt(2, "s", &["a"]), "nothing is injected");
    let (text, _) = block(&got[3]);
    assert!(text.contains("set to \"nicole\""));
    assert_eq!(
        stored(tmp.path(), "s.nicole"),
        [turn("m2.user", "user", "b"), turn("m2", "assistant", "y")]
    );
}

#[test]
fn a_turn_stays_with_the_identity_it_started_with() {
    let tmp = both();
    let mut p = start(tmp.path(), &["-i", "nicole"], 0);
    p.send(&init(), &[init_reply(json!({}))]);
    p.open(
        &request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] })),
        &[response(1, json!({ "sessionId": "s" }))],
        "s",
        json!([picker("nicole", &["nicole", "reviewer"])]),
    );
    p.send(
        &prompt(2, "s", &["question for nicole"]),
        &[chunk("s", Some("m1"), "answer as nicole")],
    );
    p.ask(
        &request(
            3,
            "session/set_config_option",
            json!({ "sessionId": "s", "configId": "identity", "value": "reviewer" }),
        ),
        1,
    );
    let cancel =
        json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": "s" } });
    p.send(&s(cancel), &[end(2)]);
    p.finish();
    assert_eq!(
        stored(tmp.path(), "s.nicole"),
        [
            turn("m1.user", "user", "question for nicole"),
            turn("m1", "assistant", "answer as nicole"),
        ]
    );
    assert!(!tmp
        .path()
        .join("reviewer/sessions/s.reviewer.turns.jsonl")
        .exists());
}

#[test]
fn an_agent_with_only_modes_gets_no_picker() {
    let tmp = both();
    let mut p = start(tmp.path(), &["-i", "nicole"], 0);
    p.send(&init(), &[init_reply(json!({}))]);
    let modes =
        json!({ "currentModeId": "ask", "availableModes": [{ "id": "ask", "name": "Ask" }] });
    let reply = response(1, json!({ "sessionId": "s", "modes": modes }));
    let got = p.call(
        &request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] })),
        std::slice::from_ref(&reply),
        2,
    );
    assert_eq!(
        parse(&got[0]),
        parse(&reply),
        "a client would hide `modes` when it gets `configOptions`"
    );
    assert_eq!(parse(&got[1]), commands("s", &[]));
    let got = p.ask(&prompt(2, "s", &["/owo\treviewer"]), 2);
    assert!(got[0].contains("now records into"), "{}", got[0]);
    assert_eq!(parse(&got[1]), parse(&end(2)));
    p.finish();
}

#[test]
fn text_a_client_adds_to_a_prompt_is_not_stored() {
    let tmp = both();
    let mut p = start(tmp.path(), &["-i", "nicole"], 0);
    p.send(&init(), &[init_reply(json!({}))]);
    p.open(
        &request(1, "session/new", json!({ "cwd": "/w", "mcpServers": [] })),
        &[response(1, json!({ "sessionId": "s" }))],
        "s",
        json!([picker("nicole", &["nicole", "reviewer"])]),
    );
    // T3 Code sends its instructions and the typed text in one block, then its own notes.
    let first = "<t3_code_instructions>\nBe kind.\n</t3_code_instructions>\n\n<user_request>\nhello thea\n</user_request>";
    let notes = "<runtime_info>You run in T3 Code.</runtime_info>\n\n<pull_request_linking>\nLink each PR.\n</pull_request_linking>";
    p.send(
        &prompt(2, "s", &[first, notes]),
        &[chunk("s", Some("m1"), "hi"), end(2)],
    );
    p.send(
        &prompt(3, "s", &["<b>bold</b> stays", notes]),
        &[chunk("s", Some("m2"), "ok"), end(3)],
    );
    let wrapped = "<t3_code_instructions>x</t3_code_instructions>\n<user_request>\n/owo reviewer\n</user_request>";
    let got = p.ask(&prompt(4, "s", &[wrapped, notes]), 3);
    assert!(got[0].contains("now records into"), "{}", got[0]);
    p.finish();
    assert_eq!(
        stored(tmp.path(), "s.nicole"),
        [
            turn("m1.user", "user", "hello thea"),
            turn("m1", "assistant", "hi"),
            turn("m2.user", "user", "<b>bold</b> stays"),
            turn("m2", "assistant", "ok"),
        ]
    );
}
