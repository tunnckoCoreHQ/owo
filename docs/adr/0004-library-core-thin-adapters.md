# Library core, thin adapters

The `owo` library crate (`src/lib.rs`) owns all behavior through one entry point, `owo::run(command) -> Output`. The CLI, the MCP server and the ACP proxy are thin adapters that call it.

The CLI parses arguments with clap, calls `owo::run`, and prints the output. An MCP tool call parses its argv with the same clap definitions and calls `owo::run` in process, so a tool and a command always behave alike. The `owo acp` command is a pass-through proxy.

This supersedes [ADR-0003](0003-mcp-runs-the-cli.md), where each MCP tool call ran the `owo` binary as a child process.
