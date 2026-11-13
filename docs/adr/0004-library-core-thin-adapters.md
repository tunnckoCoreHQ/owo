# Library core, thin adapters

The `owo` library crate (`src/lib.rs`) runs every command through one entry point, `owo::run(&cli, env) -> Output`. It prints nothing, reads no environment variables and does not exit. The CLI, the MCP server and the ACP proxy are small adapters in the binary, and they use only the library's public interface.

The CLI parses arguments with clap, calls `owo::run`, and prints the output. An MCP tool call parses its argv with the same clap definitions and calls `owo::run` in process. A tool and a command then run the same code and give the same output, a tool call starts no child process, and the server can catch a panic and answer the next request. The `owo acp` proxy copies ACP lines between a client and an agent, and stores the turns by calling `owo::run` with `sync push`.

The protocol code stays in the adapters: the MCP loop in `src/mcp.rs`, and the ACP line copying and injection in `src/acp.rs`. The library knows neither protocol.

This supersedes [ADR-0003](0003-mcp-runs-the-cli.md), where each MCP tool call ran the `owo` binary as a child process.
