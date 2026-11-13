# Decisions

Implementation decisions and how they shape current behavior. The ADRs in `docs/adr/` record the general decisions; a section links the ADR that covers it.

## Path decides kind

The entry kind comes from its path, and frontmatter `type` must agree. One source of truth for placement keeps `add`, `lint` and `index` consistent. Both directions of the map live in `identity.rs`; see [[architecture#Loading an identity]]. See [ADR-0001](../docs/adr/0001-identity-is-plain-markdown-files.md).

## Generated files carry no dates

Indexes and the timeline depend only on entry content, so `owo index --check` and the `generated` lint rule compare byte for byte. See [ADR-0001](../docs/adr/0001-identity-is-plain-markdown-files.md).

## Stamp touches frontmatter

`owo stamp` moves a file's state timestamp only when its hash changes, and then sets `updated:` in that entry's frontmatter to the local day (the day `owo add` uses) and re-hashes it.

A changed soul file bumps the minor version in `soul/soul.json`. New identities get `review.memory_hash` set at creation, so a fresh identity lints without a memory warning.

## Identifier check is literal

Identifier values are matched as exact substrings across every file. The check is simple and predictable; the cost is that very short values (under 3 characters) are skipped.

## Templates in the binary

Templates are compiled in with `include_str!` so the binary works with no install step; `<root>/.templates/` overrides them per root. See [ADR-0002](../docs/adr/0002-small-dependency-set.md).

## Search is in-crate BM25

`owo search` is about 100 lines of BM25 in `search.rs` over a few hundred files, rebuilt per call. It adds no dependency and no cache, and an identity stays small enough that a scan takes milliseconds. See [ADR-0002](../docs/adr/0002-small-dependency-set.md).

Title and aliases count three times, summary twice, body once. A query equal to a title or alias ranks that entry first. Each hit reports the body line with the most query terms (top-level headings skipped), else the best frontmatter line. Tantivy fits if identities grow past a few thousand files.

## KB pointer is always emitted

The project KB is a pointer for the agent to follow. `owo context` prints it when the path is missing too, with a warning, so a misconfigured path shows up at load time.

## Examples are built with owo

`examples/nicole` and `examples/reviewer` were scaffolded with `owo new` and `owo add`, filled by hand, then indexed, stamped and reviewed. `tests/examples.rs` checks they stay lint clean and current.

## Changes read Git, else stamps

`owo changes` reads `git log --name-status` for the identity folder when it sits in a repo, and lists uncommitted paths separately. Outside a repo it groups state-file `updated` stamps by local day, so the command always answers.

Outside a repo `--since` takes only a `YYYY-MM-DD` day and exits 1 on anything else; in a repo it takes whatever `git log --since` accepts, and a plain day starts at its midnight.

`lint --changed` keeps diagnostics whose path is uncommitted in Git or drifts from its stamp (`stamp --check`). Outside Git, files stamped today count too, so a session that ends with `stamp` still checks its own edits. The full lint still runs; only the report is filtered.

## Review window uses two cursors

Files count as changed when their stamp is newer than `last_reviewed`. Log days, events and journal count as unreviewed when their story date is after `reviewed_through`. Stamps track edits; story dates track the life.

`owo review done` sets `reviewed_through` to the newest date of a non-blank log, an event or a journal entry, not today, because story time can run behind or ahead of real time.

## Lint reuses find

Lint's link rules and orphan check call `find::raw_links` and `find::orphans`, so `owo links --orphans` and the `orphans` rule always agree.

## Skills live in the repo

`skills/` holds the four skills. `skills/identity/references/rules.md` is the one shared rules file; the other skills link to it and call owo for every deterministic step.

## Sync counts user turns

Message indexes are positions in the parsed transcript (user and assistant text only; tool calls and tool results drop out). The bookmark stores the index the last successful ingest reached, so a failed run is retried by the next `sync start`.

While a run is pending, new user turns count from its end, so the turns that follow do not start it again; the next slice still starts at the bookmark. Each `sync start` and `sync done` holds `.owo/sessions/<session_id>.lock` while it touches the bookmark; a lock older than a minute is taken over.

## Migrate is mechanical

`owo migrate v1` does only file moves and frontmatter fills; judgment work goes to a printed checklist.

It builds a fresh identity with `owo new`, copies soul files (STYLE becomes VOICE), adds missing frontmatter fields to world files (a body `## Sources` section joins `sources`, its links a lesson's `evidence`), splits `conflicts.md` by `##`, copies `sources/`, then indexes and stamps. It reports files with no home and prints the judgment work as a checklist.

Migrate never overwrites: a file whose target already exists is reported as skipped, including files under `sources/`, which is copied with its subfolders. v1 `index.md` files are dropped, since owo generates its own, and a status outside canon, disputed and superseded becomes canon. An event file named only by a date takes its slug from its title. A file whose frontmatter does not parse is reported as skipped and not written. When migrate fails partway, it removes the identity folder and the `sources/<slug>/` folder it created, so a rerun starts clean.

A v1 location folder becomes `life/locations/<slug>/`: its `overview.md`, `index.md`, `README.md` or same-named file is the overview, and every other file is a location detail whose `parent` links the overview. A loose `<x>.md` becomes an overview. World folders `cultures`, `factions`, `eras`, `history/eras` and `threads` map to their life folders.

## PNG cards without image crates

Embedding a card only appends `tEXt` chunks before `IEND`, so `export.rs` walks PNG chunks with a small CRC32 and base64 instead of an image dependency. See [ADR-0002](../docs/adr/0002-small-dependency-set.md).

## Working project versus project filter

The working project resolves from `--project`, `OWO_PROJECT`, then `.owo.toml`; `context`, `add note`, `state` and `sync` use it. `search` and `export card` narrow to a project only with an explicit `--project` flag.

A filter hides results, so it comes from the command line and never from the environment. Inside a project checkout, `owo search` still finds life and soul entries, and `owo export card` still exports the whole identity.

## MCP calls the library

`owo mcp` is a newline-delimited JSON-RPC loop in `mcp.rs` with no MCP crate. Each tool call builds the matching argv, parses it with `Cli::try_parse_from`, and calls `owo::run` in process; it starts no child process. See [ADR-0004](../docs/adr/0004-library-core-thin-adapters.md) and [ADR-0002](../docs/adr/0002-small-dependency-set.md).

A tool call goes through the same `env` and `render` functions in `main.rs` as a command, so a tool and a command give the same output. The `env` function builds `Env` from the process, and the MCP server passes the tool's stdin text into it. The `render` function turns an `Output` into its stdout text and stderr text; the binary prints the two, and a tool returns stdout, or stderr when stdout is blank, or both joined with a newline. A clap parse error gives the text clap prints for it.

The tool list is read from the clap definitions (`Cli::command()`): each subcommand is a tool, each flag a parameter with its help text, type and allowed values, so a new flag needs no MCP change.

Most tools add `--json` and return the envelope; `context` and `show` return text, which is what an agent reads. Positional values go after `--` and flag values are passed as `--flag=value`, so a value never parses as a flag. A non-zero exit code sets `isError`; an unknown tool name gets a -32602 error. `initialize` answers the client's protocol version when it is one the server speaks (2025-06-18, 2025-03-26, 2024-11-05), else 2025-06-18. A line that is not UTF-8 JSON gets a -32700 parse error and the server goes on.

Each tool call runs inside `std::panic::catch_unwind`. A panic gives an `isError` result with the text `owo panicked: <message>`, and the server answers the next request, on stdio and with `--remote`.

A tool call runs in the server process, so the server keeps its own copies of stdin and stdout for the protocol. The copies are close-on-exec, so a `git` child does not inherit them. It points the process stdin at `/dev/null` and the process stdout at stderr, so a tool path such as `/dev/stdin` or `/dev/stdout` cannot reach the protocol stream. A path that names a protocol descriptor by number, such as `/dev/fd/5`, still reaches it. A tool argument with a NUL character gives an `isError` result, because no command line can hold one.

The `--remote` mode is a small HTTP/1.1 loop over `std::net`: one request at a time, `POST /mcp` with a JSON reply, 202 for notifications, 405 for other methods. It has no authentication, so whoever reaches the URL can read and write the identity. To keep web pages out, it answers 403 to a request with an `Origin` header and 415 to a POST without `Content-Type: application/json`, and 411 to a POST without `Content-Length` or with any `Transfer-Encoding` header. One request gets 30 seconds to arrive and its reply 30 seconds to leave; headers are capped at 16KB (431) and the body at 8MB (413). A refused body it did not read is discarded after the reply, so the client sees the whole status; the discard stops after 200ms with nothing arriving, or after 2 seconds, so an idle client cannot hold the server.

## ACP proxy passes lines through

The `owo acp` command is a newline-delimited JSON-RPC proxy in `acp.rs` with no ACP crate and no conductor, on `std` threads. One thread copies the client's stdin to the agent, the main thread copies the agent's stdout to the client, and a third thread stores turns, so a store never delays a line. The agent's stderr is inherited. When the client closes stdin, the proxy closes the agent's stdin; the proxy ends when the agent's stdout closes, and exits with the agent's code. The proxy ignores SIGPIPE, so an agent that closes its stdin early does not kill it. When the client closes its stdout, the proxy stops the agent, finishes the stores and exits with an I/O error. See [ADR-0004](../docs/adr/0004-library-core-thin-adapters.md).

A line the proxy does not change is written as the bytes it read, in order, also when it is not JSON or names an unknown method. A line the proxy changes is parsed and written again as one JSON line. A line the JSON parser refuses, such as one with a lone surrogate escape, passes the same way and is not read for turns. At start the proxy resolves an identity (from `-i`, `OWO_IDENTITY` or the config, read with `owo config --json` in process) and reads the identities of the root with `owo list --json`. When the root has no identity, the proxy changes nothing and stores nothing. An identity with no folder, or a project that is not found, is a config error (exit 3) before the agent starts.

Each session has its own identity. It starts as the identity resolved at start, or none when no identity resolved; with none the proxy injects nothing and stores nothing for that session. A pick of the current identity changes nothing. The client switches it with a session config option or the `/owo` command, and the proxy answers both itself, without waiting for the agent. Both threads write to the client through the stdout lock, so each line is written whole, and the agent's lines keep their order.

The picker is a `select` config option `{"id": "identity", "name": "Identity", "category": "_owo", ...}` whose values are `_none` and the slug of each identity, named with the identity name. The proxy adds it after the agent's own options to the `configOptions` of each response to `session/new`, `session/load` and `session/resume` (it creates the array when there is none), of each `session/set_config_option` result it forwarded, and of each `config_option_update`. It keeps the agent's last list of each session. A `session/set_config_option` with `configId` `identity` does not reach the agent: the proxy answers with the agent's last list and the picker with the new value. A value that is not `_none` or a slug gets a `-32602` error, and nothing changes. A `session/set_config_option` for another option passes unchanged.

The `/owo` command is for clients with no config option UI. The proxy adds `{"name": "owo", "description": "Switch the owo identity of this session", "input": {"hint": "identity"}}` to the `availableCommands` of each `available_commands_update` and keeps the agent's last list of each session. Right after the response to `session/new`, `session/load` or `session/resume`, it sends its own `available_commands_update` with the agent's last list and `owo`. A `session/prompt` whose first text block is `/owo` or starts with `/owo ` does not reach the agent and is not stored. With `/owo <slug>` or `/owo none` the proxy switches the identity; with `/owo` alone or an unknown name it switches nothing. It answers with one `agent_message_chunk` line that names the new identity, or the current one and the names to choose from; then, when the identity changed, a `config_option_update` with the full list; then the response with `stopReason` `end_turn`.

The proxy gives the identity to the agent in one of two ways. By default it adds `{"name": "owo", "command": "owo", "args": ["mcp", "-i", <identity>], "env": []}` to `mcpServers` of `session/new`, `session/load` and `session/resume`, so the agent finds `owo` on its `PATH`; with no identity resolved at start the `args` are `["mcp"]`. The text block tells the agent to load the identity with the `context` tool of that server, with the argument `identity` set to the session's identity, because that identity can change after the agent started the server. A request that already has a server named `owo` keeps only that one. With `--inject` it adds no server, and the text block is the output of `owo -i <identity> context`, read when a prompt gets the block, so the agent needs no tool call; when that command fails, the proxy prints one warning line and injects nothing. With `--inject` and no identity in the root, `owo acp` is a usage error (exit 2).

The text block goes first in the `prompt` of the first `session/prompt` of a session from `session/new` that has an identity, and of the first prompt after a switch to another identity. A switch to none injects nothing, and the stored user turn leaves the block out. When the agent answers a prompt that carried the block with an error, the next prompt carries it. A loaded or resumed session gets no block until a switch. The proxy does not use the ACP `systemPrompt` field: the spec made it stable on 2026-11-06, and agents do not advertise it yet. A client's own `systemPrompt` passes unchanged.

Turns come from the text blocks of `session/prompt` (joined with a newline) and from `agent_message_chunk` updates with text content; chunks with the same `messageId` are one message, and chunks with no `messageId` are one message until another update, such as a tool call, comes between them. Other updates, `user_message_chunk` among them, are ignored. A session can have more than one open request, for example a new `session/prompt` sent before the cancelled one answers; an update belongs to the oldest open one. The proxy stores a turn at the response to its `session/prompt`, and the replayed messages at the response to `session/load` when there are any, under the session's identity at that time, through `owo -i <identity> sync push` with `<sessionId>.<identity>` as the session, so the turns of two identities never share a store file; a failed store is one `owo: warning: <text>` line. When the agent exits, the proxy stores the turns that are still open. A `sessionId` outside the letters, digits, `-`, `_` and `.` that `sync push` takes fails each store this way. The proxy only stores turns: it gives `sync push` a run size that is never reached, so no ingest run starts and no slice is written. A host starts a run for the session with `owo -i <identity> sync push --session <sessionId>.<identity> --turns '[]'`.

Turn ids: an agent message's id is its `messageId`. ACP gives `session/prompt` no message id, so the user turn's id is the first agent `messageId` of that turn plus `.user`. A live message with no `messageId`, or a user turn with no agent `messageId`, gets an id from the clock, so it is stored once. A replayed chunk with no `messageId` is not stored, because it cannot be told from a stored one. A replayed message with a stored `messageId` adds nothing, because `sync push` skips stored ids.
