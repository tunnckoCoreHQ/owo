//! One test per open finding from the full review. Each fails until its bug is fixed.

mod common;

use common::{json, owo};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use tempfile::TempDir;

fn thea() -> TempDir {
    let tmp = TempDir::new().unwrap();
    owo(tmp.path()).args(["new", "Thea"]).assert().success();
    tmp
}

fn mcp(root: &Path, messages: &[Value]) -> Vec<Value> {
    let input: String = messages.iter().map(|m| format!("{m}\n")).collect();
    let out = owo(root)
        .args(["-i", "thea", "mcp"])
        .write_stdin(input)
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn messages(v: &Value, key: &str) -> String {
    v[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| format!("{} {}\n", d["path"], d["message"]))
        .collect()
}

#[test]
fn mcp_initialize_answers_a_version_it_supports() {
    let tmp = thea();
    let init = |id: u64, version: &str| {
        json!({ "jsonrpc": "2.0", "id": id, "method": "initialize",
            "params": { "protocolVersion": version, "capabilities": {}, "clientInfo": { "name": "t", "version": "1" } } })
    };
    let replies = mcp(tmp.path(), &[init(1, "1999-01-01"), init(2, "2025-06-18")]);
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[1]["result"]["protocolVersion"], "2025-06-18");
}

#[test]
fn mcp_unknown_tool_is_an_invalid_params_error() {
    let tmp = thea();
    let call = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": "no_such_tool", "arguments": {} } });
    let replies = mcp(tmp.path(), &[call]);
    assert_eq!(replies[0]["error"]["code"], -32602, "{}", replies[0]);
}

#[test]
fn mcp_remote_refuses_a_body_without_content_length() {
    let tmp = thea();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_owo"))
        .env_remove("OWO_ROOT")
        .env_remove("OWO_IDENTITY")
        .env_remove("OWO_PROJECT")
        .arg("--root")
        .arg(tmp.path())
        .args(["-i", "thea", "mcp", "--remote", "--port", "0"])
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
    let mut s = std::net::TcpStream::connect(&host).unwrap();
    write!(
        s,
        "POST /mcp HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n"
    )
    .unwrap();
    let mut reply = String::new();
    s.read_to_string(&mut reply).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(reply.starts_with("HTTP/1.1 411"), "{reply}");
}

#[test]
fn card_greetings_skip_headings_in_code_fences() {
    let tmp = thea();
    let dir = tmp.path().join("thea");
    fs::write(
        dir.join("soul/examples/greetings.md"),
        "# Greetings\n\n## Hello\n\nHi there.\n\n```md\n## Not a greeting\n```\n",
    )
    .unwrap();
    let out = owo(tmp.path())
        .args(["-i", "thea", "export", "card"])
        .output()
        .unwrap();
    let card: Value = serde_json::from_slice(&out.stdout).unwrap();
    let first = card["data"]["first_mes"].as_str().unwrap();
    assert!(first.contains("## Not a greeting"), "{first}");
    assert_eq!(card["data"]["alternate_greetings"], json!([]));
}

#[test]
fn slug_with_no_letters_says_why() {
    let tmp = TempDir::new().unwrap();
    let v = json(owo(tmp.path()).args(["slug", "!!!"]));
    assert_eq!(v["ok"], false);
    assert!(!v["errors"].as_array().unwrap().is_empty(), "{v}");
}

#[test]
fn titles_that_break_links_are_refused() {
    let tmp = thea();
    let v = json(owo(tmp.path()).args(["-i", "thea", "add", "character", "C# Dev"]));
    assert_eq!(v["ok"], false, "{v}");
    let dir = tmp.path().join("thea");
    owo(tmp.path())
        .args(["-i", "thea", "add", "character", "Pipe"])
        .assert()
        .success();
    let file = dir.join("life/characters/pipe.md");
    let text = fs::read_to_string(&file).unwrap();
    fs::write(&file, text.replace("title: \"Pipe\"", "title: \"Pi|pe\"")).unwrap();
    let v = json(owo(tmp.path()).args(["-i", "thea", "lint", "--rule", "schema"]));
    let errors = messages(&v, "errors");
    assert!(errors.contains("life/characters/pipe.md"), "{errors}");
}

#[test]
fn a_fence_closes_only_on_a_run_at_least_as_long() {
    let tmp = thea();
    owo(tmp.path())
        .args(["-i", "thea", "add", "character", "Fence"])
        .assert()
        .success();
    let file = tmp.path().join("thea/life/characters/fence.md");
    let mut text = fs::read_to_string(&file).unwrap();
    text.push_str("\n````md\n```\n[[Inside Fence]]\n```\n````\n");
    fs::write(&file, text).unwrap();
    let v = json(owo(tmp.path()).args(["-i", "thea", "lint", "--rule", "links"]));
    let errors = messages(&v, "errors");
    assert!(!errors.contains("Inside Fence"), "{errors}");
}

fn add(root: &Path, args: &[&str]) {
    owo(root)
        .args(["-i", "thea", "add"])
        .args(args)
        .assert()
        .success();
}

fn card(root: &Path) -> Value {
    let out = owo(root)
        .args(["-i", "thea", "export", "card"])
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn a_blank_log_does_not_hide_recent_events() {
    let tmp = thea();
    add(tmp.path(), &["event", "Storm", "--date", "2020-05-01"]);
    add(tmp.path(), &["log", "--date", "2020-05-03"]);
    let log = tmp.path().join("thea/life/log/2020-05-03.md");
    let text = fs::read_to_string(&log).unwrap();
    fs::write(&log, format!("{text}\n- Rowed out.\n")).unwrap();
    add(tmp.path(), &["log", "--date", "2020-06-01"]);
    let out = owo(tmp.path())
        .args(["-i", "thea", "state"])
        .output()
        .unwrap();
    let state = String::from_utf8(out.stdout).unwrap();
    assert!(state.contains("Recent events: [[Storm]]"), "{state}");
}

#[test]
fn an_empty_key_facts_section_falls_back_to_the_body() {
    let tmp = thea();
    add(tmp.path(), &["character", "Jay Doe"]);
    let file = tmp.path().join("thea/life/characters/jay-doe.md");
    let text = fs::read_to_string(&file).unwrap();
    let text = text.replacen(
        "## Description\n",
        "## Description\n\nJay runs the harbor bar.\n",
        1,
    );
    assert!(
        text.contains("Jay runs"),
        "the template has a Description section"
    );
    fs::write(&file, text).unwrap();
    let card = card(tmp.path());
    let entries = card["data"]["character_book"]["entries"]
        .as_array()
        .unwrap();
    let jay = entries.iter().find(|e| e["name"] == "Jay Doe").unwrap();
    assert!(
        jay["content"].as_str().unwrap().contains("Jay runs"),
        "{jay}"
    );
}

#[test]
fn context_with_loads_each_file_once() {
    let tmp = thea();
    add(tmp.path(), &["character", "Jay Doe"]);
    let out = owo(tmp.path())
        .args(["-i", "thea", "context", "--format", "paths"])
        .args([
            "--with", "IDENTITY", "--with", "Jay Doe", "--with", "jay-doe",
        ])
        .output()
        .unwrap();
    let paths = String::from_utf8(out.stdout).unwrap();
    assert_eq!(paths.matches("soul/IDENTITY.md").count(), 1, "{paths}");
    assert_eq!(
        paths.matches("life/characters/jay-doe.md").count(),
        1,
        "{paths}"
    );
}

#[test]
fn lorebook_lists_locations_before_events() {
    let tmp = thea();
    add(tmp.path(), &["character", "Jay Doe"]);
    add(tmp.path(), &["event", "Storm", "--date", "2020-05-01"]);
    add(tmp.path(), &["location", "Harbor"]);
    let card = card(tmp.path());
    let names: Vec<&str> = card["data"]["character_book"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
    assert!(
        at("Jay Doe") < at("Harbor") && at("Harbor") < at("Storm"),
        "{names:?}"
    );
}

#[test]
fn context_paths_put_the_now_block_after_the_core_files() {
    let tmp = thea();
    let out = owo(tmp.path())
        .args(["-i", "thea", "context", "--format", "paths", "--voice"])
        .output()
        .unwrap();
    let paths = String::from_utf8(out.stdout).unwrap();
    let tiers: Vec<&str> = paths
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let now = tiers.iter().position(|t| *t == "now").unwrap();
    assert!(tiers[..now].iter().all(|t| *t == "core"), "{paths}");
    assert!(now < tiers.len() - 1, "{paths}");
}
