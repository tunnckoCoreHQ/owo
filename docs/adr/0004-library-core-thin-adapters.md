# Library core, thin adapters

The `owo` library crate (`src/lib.rs`) holds every feature. It runs each command through one entry point, `owo::run(&cli, &env, stdin) -> Output`, and it also holds the MCP server (`owo::mcp`) and the ACP proxy (`owo::acp`). The `owo` binary is one consumer of the library.

The library reads nothing from the process. It reads no environment variable, takes no stream of the process, prints nothing, does not exit and starts no agent. The caller passes in the environment values as `Env`, the stdin reader of a command, the streams of the MCP server and the ACP proxy, a writer for warning lines, and options such as `owo::acp::Options`. So a program that links the library can do everything `owo`, `owo mcp` and `owo acp` do, on its own streams, and the library tests run the server and the proxy on in-memory streams with no binary.

The binary only connects the process to the library: it parses the arguments, builds `Env` from the environment, hands over stdio (for `owo mcp` it keeps copies of stdin and stdout and points the originals away, so a tool path cannot reach the protocol), starts the agent process for `owo acp`, prints the `Output`, and returns the exit code.

An MCP tool call parses its argv with the same clap definitions and calls `owo::run` in process. A tool and a command then run the same code and give the same output, a tool call starts no child process, and the server can catch a panic and answer the next request. The ACP proxy stores the turns by calling `owo::run` with `sync push`. The rendering of an `Output` as stdout and stderr text is in the library, because the MCP tool result is built from it.

This supersedes [ADR-0003](0003-mcp-runs-the-cli.md), where each MCP tool call ran the `owo` binary as a child process.
