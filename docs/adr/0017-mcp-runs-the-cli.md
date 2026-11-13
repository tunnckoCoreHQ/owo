# MCP runs the CLI

The `owo mcp` command is a JSON-RPC loop with no MCP crate. Each tool call runs the same `owo` binary, so a tool and a command always behave alike. The tool list comes from the clap definitions, so a new flag needs no MCP change.
