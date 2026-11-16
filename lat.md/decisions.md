# Decisions

Implementation decisions and how they shape current behavior. The ADRs in `docs/adr/` record the general decisions; a section links the ADR that covers it.

## Path decides kind

The entry kind comes from its path, and frontmatter `type` must agree. One source of truth for placement keeps `add`, `lint` and `index` consistent. Both directions of the map live in `identity.rs`; see [[architecture#Loading an identity]]. See [ADR-0001](../docs/adr/0001-identity-is-plain-markdown-files.md).

## Generated files carry no dates

Indexes and the timeline depend only on entry content, so `owo index --check` and the `generated` lint rule compare byte for byte. See [ADR-0001](../docs/adr/0001-identity-is-plain-markdown-files.md).

## Stamp touches frontmatter

`owo stamp` moves a file's stamp only when its hash changes, and then sets `updated:` in that entry's frontmatter to the local day (the day `owo add` uses) and re-hashes it.

A changed persona file bumps the minor version in `persona/soul.json`. New identities get `reflection.memory_hash` set at creation, so a fresh identity lints without a memory warning.

## Identifier check is literal

Identifier values are matched as exact substrings across every file. The check is simple and predictable; the cost is that very short values (under 3 characters) are skipped.

## Templates in the binary

Templates are compiled in with `include_str!` so the binary works with no install step; `<root>/.templates/` overrides them per root. See [ADR-0002](../docs/adr/0002-small-dependency-set.md).

## Search is in-crate BM25

`owo search` is about 100 lines of BM25 in `search.rs` over a few hundred files, rebuilt per call. It adds no dependency and no cache, and an identity stays small enough that a scan takes milliseconds. See [ADR-0002](../docs/adr/0002-small-dependency-set.md).

Title and aliases count three times, summary twice, body once. A query equal to a title or alias ranks that entry first. Each hit reports the body line with the most query terms (top-level headings skipped), else the best frontmatter line. Tantivy fits if identities grow past a few thousand files.

## Project wiki path is always emitted

The project wiki path is for the agent to follow. `owo context` prints it when the path is missing too, with a warning, so a misconfigured path shows up at load time.

## Persona prints persona files without the load set

An agent loads the identity once, at the start of a thread, with `owo context`. When it needs one persona file later, such as LOOK for an image, `owo persona --look` prints that file alone, so the agent does not load the whole set again. With no flag, `owo persona` prints IDENTITY and SOUL, the two files that say who the identity is.

The flag for IDENTITY is `--id`, not `--identity`. The global `-i/--identity` flag and the MCP `identity` argument already name the selected identity, and clap cannot give one command two `--identity` flags.

## Examples are built with owo

`examples/nicole` and `examples/reviewer` were scaffolded with `owo new` and `owo add`, filled by hand, then indexed and stamped, and a first reflection was recorded. `crates/cli/tests/examples.rs` checks they stay lint clean and current.

## Changes read Git, else stamps

`owo changes` reads `git log --name-status` for the identity folder when it sits in a repo, and lists uncommitted paths separately. Outside a repo it groups state-file `updated` stamps by local day, so the command always answers.

Outside a repo `--since` takes only a `YYYY-MM-DD` day and exits 1 on anything else; in a repo it takes whatever `git log --since` accepts, and a plain day starts at its midnight. Git reads a day only from 1970 to 2099, so a day before 1970-01-02 lists every commit and a day from 2100 lists none, and owo does not pass either day to Git.

`lint --changed` keeps diagnostics whose path is uncommitted in Git or drifts from its stamp (`stamp --check`). Outside Git, files stamped today count too, so a session that ends with `stamp` still checks its own edits. The full lint still runs; only the report is filtered.

## Reflection window uses two cursors

Files count as changed when their stamp is newer than `last_reflected`. Daily logs, events and journal count as new for a reflection when their story date is after `reflected_through`. Stamps track edits; story dates track the life.

`owo reflect done` sets `reflected_through` to the newest date of a non-blank daily log, an event or a journal entry, not today, because story time can run behind or ahead of real time.

## Lint reuses find

Lint's wikilink rules and orphan check call `find::raw_wikilinks` and `find::orphans`, so `owo links --orphans` and the `orphans` rule always agree.

## Skills live in the repo

`skills/owo/` holds the one `owo` skill. Its short entry file `SKILL.md` sends the agent to the path for the task, each in `skills/owo/references/`: `embody.md`, `create.md`, `ingest.md` and `reflect.md`. `skills/owo/references/rules.md` is the one rules file that every path that writes follows; each path calls owo for every deterministic step. One skill with paths loads one description into the host and keeps the shared rules beside the paths that use them. The entry file holds the checkpoint under its own heading, so every path names it. `skills/owo/references/glossary.md` holds the terms the skill uses; it is a relative symlink to `GLOSSARY.md`, so the README says to link the whole skill folder or to copy it with `cp -rL`. The four older skills stay in `_skills/` for reference; nothing links to them.

## Sync counts user turns

Turn indexes are positions in the parsed transcript (user and assistant text only; tool calls and tool results drop out). The sync cursor stores the index the last successful ingest reached, so a failed run is retried by the next `sync start`.

While a run is pending, new user turns count from its end, so the turns that follow do not start it again; the next slice still starts at the sync cursor. Each `sync start` and `sync done` holds `.owo/sessions/<session_id>.lock` while it touches the sync cursor; a lock older than a minute is taken over.

## Migrate is mechanical

`owo migrate v1` does only file moves and frontmatter fills; judgment work goes to a printed checklist.

It builds a fresh identity with `owo new`, copies persona files (STYLE becomes VOICE; `crates/cli/tests/migrate.rs` checks that VOICE holds the v1 STYLE text, since `owo new` makes a VOICE of its own), adds missing frontmatter fields to world files (a body `## Sources` section joins `sources`, its links an insight's `evidence`), splits `conflicts.md` by `##`, copies `sources/`, then indexes and stamps. It reports files with no home and prints the judgment work as a checklist.

Migrate never overwrites: a file whose target already exists is reported as skipped, including files under `sources/`, which is copied with its subfolders. v1 `index.md` files are dropped, since owo generates its own, and a status outside canon, disputed and superseded becomes canon. An event file named only by a date takes its slug from its title. A file whose frontmatter does not parse is reported as skipped and not written. When migrate fails partway, it removes the identity folder and the `sources/<slug>/` folder it created, so a rerun starts clean.

A v1 location folder becomes `life/locations/<slug>/`: its `overview.md`, `index.md`, `README.md` or same-named file is the overview, and every other file is a location detail whose `parent` links the overview. A loose `<x>.md` becomes an overview. World folders `cultures`, `factions`, `eras`, `history/eras` and `threads` map to their life folders.

## PNG persona cards without image crates

Embedding a persona card only appends `tEXt` chunks before `IEND`, so `export.rs` walks PNG chunks with a small CRC32 and base64 instead of an image dependency. See [ADR-0002](../docs/adr/0002-small-dependency-set.md).

## Working project versus project filter

The working project resolves from `--project`, `OWO_PROJECT`, then `.owo.toml`; `context`, `add project-note`, `state` and `sync` use it. `search` and `export persona-card` narrow to a project only with an explicit `--project` flag.

A filter hides results, so it comes from the command line and never from the environment. Inside a project checkout, `owo search` still finds life and persona entries, and `owo export persona-card` still exports the whole identity.

`sync due` follows the same rule for the identity: it lists the runs of one identity only with `-i` (or the MCP `identity`), and every run in the root otherwise. The filter reads the slice path of each pending run, `sources/<identity>/...`, so the sync cursor needs no identity field.

## Options are declared once in the core

Each request type in `crates/core/src/request.rs` reads itself with `Request::read`, and each read declares one field with its name, type, help text, default, allowed values and rules, so a field name is written once. The `Reader` method of the read sets the field type, such as a day for `Reader::day`, a switch for `Reader::flag` and a JSON array for `Reader::json`, so the type is written once too. `Reader::words` also makes its field positional, and `Field::positional` marks the other positionals. A `Field` takes only its name and help text. The CLI builds its flags from the fields and the MCP server its parameters, and both hand their arguments to the core as JSON values, so a new option is one new read. A field name of two words is in camelCase, which is also its MCP parameter name, and the CLI writes its flag in kebab-case: the field `dryRun` is the flag `--dry-run`. The rules (`range`, `conflicts`, `requires`) are declared once, on the fields. Each typed function checks its request against them with `Request::check`, which reads the request through the same declarations, so a program that builds a request by hand, the MCP server and the CLI all get the same check, and a broken rule is a usage error, never a crash. The CLI also gives the declarations to clap, which checks them again before the call, so a bad flag keeps clap's error and usage text. See [ADR-0005](../docs/adr/0005-typed-library-api.md).

The core's error text and field help name fields, such as `until` or `out`, and no flag, environment variable, command or exit code. The CLI words a typed `Fault` in its own terms: the flag (`--until`), the `owo` command to run next, and the help lines that name stdin, exit codes or other flags.

A path argument need not be UTF-8: the CLI hands the core the path as it got it (`Args::with_path`), and each typed function that takes a path turns it into UTF-8 text, each run of bytes that is not UTF-8 becoming U+FFFD, before it reads or writes. The CLI, the MCP server, the ACP proxy and a program that links the core then reach the same file for the same argument.

## MCP calls the library

The MCP server is `owo_core::mcp` in the core, a newline-delimited JSON-RPC loop with no MCP crate. It is a consumer of the operations, not of the CLI: each tool is an operation of `operations()`, and a tool call maps its JSON arguments into the operation's request, runs it, and returns the outcome. It parses nothing with clap, runs no CLI code and starts no child process. See [ADR-0005](../docs/adr/0005-typed-library-api.md) and [ADR-0002](../docs/adr/0002-small-dependency-set.md).

The caller builds `Env` once and gives it to the server, with the server's own root, identity and project as a `Select`; the `identity` and `project` arguments of a call replace the last two. A tool and a command with `--json` give the same envelope, from `Outcome::envelope`. The function `mcp::reply` answers one message; `serve` runs it on a reader and a writer, and `serve_http` on a listener.

The tool list and each input schema come from the request types: each field is a parameter with its help text, type and allowed values, so a new field needs no change in `mcp.rs`.

The server refuses an unknown argument, a missing required one, a value outside a fixed list (`invalid value "x" for "type"; possible values: ...`), and a value that is not an integer or a `YYYY-MM-DD` day, with its own text and no envelope. An argument it leaves out takes the library's default, from the request type. The typed call checks the rules of the fields (an empty required path, days outside 0 to 36500, `since` with `days`, `png` without `out`) and gives its usage error, as the library text in a fail envelope, not in clap's usage format. Either error sets `isError`. A string argument takes a number or a boolean as its text, and an integer argument takes a string of digits. A single-value argument also takes an array of one value (`{"scope": ["persona"]}`, `{"limit": [5]}`); an array of more values is an error that names the type of the field, such as `argument "days" must be an integer`. A positional argument of several words is a string, or an array of words joined with spaces. A JSON field, such as `turns`, reaches the request as its JSON text, as the CLI gives it.

Most tools return the envelope followed by the warning lines, such as the one for a root from a `.owo.toml` in a parent folder; `context`, `show` and `persona` return text, which is what an agent reads, followed by the warning lines, or the warning lines and the error line `owo: <error>` when the call fails. An error text is the core's plain text, with no `owo` command to run; the CLI adds those hints. A failed call or an envelope with `ok` false sets `isError`; an unknown tool name gets a -32602 error. `initialize` answers the client's protocol version when it is one the server speaks (2025-06-18, 2025-03-26, 2024-11-05), else 2025-06-18. A line that is not UTF-8 JSON gets a -32700 parse error and the server goes on.

Each tool call runs inside `std::panic::catch_unwind`. A panic gives an `isError` result with the text `owo panicked: <message>`, and the server answers the next request, on stdio and with `--remote`.

A tool call runs in the server process, so for `owo mcp` the binary keeps its own copies of stdin and stdout for the protocol and gives them to `serve`. The copies are close-on-exec, so a `git` child does not inherit them. The binary points the process stdin at `/dev/null` and the process stdout at stderr, so a tool path such as `/dev/stdin` or `/dev/stdout` cannot reach the protocol stream. A path that names a protocol descriptor by number, such as `/dev/fd/5`, still reaches it. A tool argument other than `turns` with a NUL character gives an `isError` result, as a command line cannot hold one.

The `--remote` mode is a small HTTP/1.1 loop over `std::net` in `serve_http`. The binary opens the listener with `bind`, prints its URL, then detaches its stdin and stdout the same way. The loop serves one request at a time, `POST /mcp` with a JSON reply, 202 for notifications, 405 for other methods. It has no authentication, so whoever reaches the URL can read and write the identity. To keep web pages out, it answers 403 to a request with an `Origin` header and 415 to a POST without `Content-Type: application/json`, and 411 to a POST without `Content-Length` or with any `Transfer-Encoding` header. One request gets 30 seconds to arrive and its reply 30 seconds to leave; headers are capped at 16KB (431) and the body at 8MB (413). A refused body it did not read is discarded after the reply, so the client sees the whole status; the discard stops after 200ms with nothing arriving, or after 2 seconds, so an idle client cannot hold the server.

## ACP proxy passes lines through

The ACP proxy is `owo_core::acp` in the core, a newline-delimited JSON-RPC proxy with no ACP crate and no conductor, on `std` threads. `Proxy::run` takes four streams: client input, client output, agent input and agent output. One thread copies client input to the agent, the calling thread copies agent output to the client, and a third thread stores turns, so a push never delays a line. When client input ends, the proxy closes agent input. When agent output ends, or a write to the client fails, the proxy stores the turns of the prompts that are still open and returns, with the error in the second case. Warning lines go to the writer given to `Proxy::new`; a failed push or context read is one warning line with the library text, which names no CLI command. For `owo acp` the binary starts the agent as a child process with its stderr inherited, passes the process stdin, stdout and stderr, and exits with the agent's code; when the client closed its stdout, it stops the agent and exits with an I/O error. The binary ignores SIGPIPE, so an agent that closes its stdin early does not kill it. The proxy uses the typed library calls and builds no command line, so a program that links the core can run it on its own streams. See [ADR-0005](../docs/adr/0005-typed-library-api.md).

A line the proxy does not change is written as the bytes it read, in order, also when it is not JSON or names an unknown method. A line the proxy changes is parsed and written again as one JSON line. A line the JSON parser refuses, such as one with a lone surrogate escape, passes the same way and is not read for turns. At start the proxy resolves an identity (from `-i`, `OWO_IDENTITY` or the config) with `Root::open`, loads it and checks the project, and lists the identities of the root with `Root::list`. It writes the config warnings as warning lines. When the root has no identity, the proxy changes nothing and stores nothing. An identity with no folder, or a project that is not found, is a config error (exit 3) before the agent starts.

Each session has its own identity. It starts as the identity resolved at start, or none when no identity resolved; with none the proxy injects nothing and stores nothing for that session. A pick of the current identity changes nothing. The client switches it with a session config option or the `/owo` command, and the proxy answers both itself, without waiting for the agent. Both threads write to the client through one lock on the client writer, so each line is written whole, and the agent's lines keep their order.

The picker is a `select` config option `{"id": "identity", "name": "Identity", "category": "_owo", ...}` whose values are `_none` and the slug of each identity, named with the identity name. The proxy adds it after the agent's own options to the `configOptions` of each response to `session/new`, `session/load` and `session/resume` (it creates the array when there is none; a response with the older `modes` field and no `configOptions` gets no picker, because a client that gets `configOptions` hides `modes`), of each `session/set_config_option` result it forwarded, and of each `config_option_update`. It keeps the agent's last list of each session. A `session/set_config_option` with `configId` `identity` does not reach the agent: the proxy answers with the agent's last list and the picker with the new value. A value that is not `_none` or a slug gets a `-32602` error, and nothing changes. A `session/set_config_option` for another option passes unchanged.

The `/owo` command is for clients with no config option UI. The proxy adds `{"name": "owo", "description": "Switch the owo identity of this session", "input": {"hint": "identity"}}` to the `availableCommands` of each `available_commands_update` and keeps the agent's last list of each session. Right after the response to `session/new`, `session/load` or `session/resume`, it sends its own `available_commands_update` with the agent's last list and `owo`. A `session/prompt` whose first text block is `/owo`, alone or followed by white space and an argument, does not reach the agent and is not stored. With `/owo <slug>` or `/owo none` the proxy switches the identity; with `/owo` alone or an unknown name it switches nothing. It answers with one `agent_message_chunk` line that names the new identity, or the current one and the names to choose from; then, when the identity changed, a `config_option_update` with the full list; then the response with `stopReason` `end_turn`.

The proxy gives the identity to the agent in one of two ways. By default it adds an MCP server named `owo` to `mcpServers` of `session/new`, `session/load` and `session/resume`. The caller gives its command line in `Options::mcp_server`, from the identity resolved at start, so the core knows no binary name or flag. For `owo acp` it is `{"name": "owo", "command": <path of the owo binary>, "args": ["mcp", "-i", <identity>], "env": []}`, so the agent starts the proxy's own binary, not the `owo` on its `PATH`; the command is `owo` only when the binary path is unknown; with no identity resolved at start the `args` are `["mcp"]`. The identity block, the text the proxy puts first in a prompt, tells the agent to load the identity with the `context` tool of that server, with the argument `identity` set to the session's identity, because that identity can change after the agent started the server. A request that already has a server named `owo` keeps only that one. With `inject` set in `Options` (the `--inject` flag) it adds no server, and the identity block is the text `owo -i <identity> context` prints, read with `Identity::context` and `context_text` when a prompt gets the block, so the agent needs no tool call; when that call fails, the proxy writes one warning line and injects nothing. With `inject` and no identity in the root, `Proxy::new` returns a usage error, and `owo acp` exits 2.

The identity block goes first in the `prompt` of the first `session/prompt` of a session from `session/new` that has an identity, and of the first prompt after a switch to another identity. A switch to none injects nothing, and the stored user turn leaves the block out. When the agent answers a prompt that carried the block with an error, the next prompt carries it. A loaded or resumed session gets no block until a switch. The proxy does not use the ACP `systemPrompt` field: the spec made it stable on 2026-11-06, and agents do not advertise it yet. A client's own `systemPrompt` passes unchanged.

Turns come from the typed text of `session/prompt` (the text blocks, joined with a newline). A client such as T3 Code adds its own instructions to a prompt as XML-like elements, so the proxy takes the content of a `<user_request>` element as the typed text and leaves out a block made only of elements whose names have an underscore, such as `<runtime_info>`. The `/owo` command is read from the typed text too. Assistant turns come from `agent_message_chunk` updates with text content; chunks with the same `messageId` are one turn, and chunks with no `messageId` are one turn until another update, such as a tool call, comes between them. Other updates, `user_message_chunk` among them, are ignored. A session can have more than one open request, for example a new `session/prompt` sent before the cancelled one answers; an update belongs to the oldest open one. The proxy stores the turns of a prompt at the response to its `session/prompt`, and the replayed turns at the response to `session/load` when there are any, for the identity the session had when the prompt or the load was sent, with `SessionSync::push` (what `owo -i <identity> sync push` runs) and `<sessionId>.<identity>` as the session, so the turns of two identities never share a turns file; a failed push is one `owo: warning: <text>` line. When the agent exits, the proxy stores the turns of the prompts that are still open. A `sessionId` outside the letters, digits, `-`, `_` and `.` that `sync push` takes fails each push this way. The proxy syncs like every other caller: it gives `sync push` the default run size (`sync_every` from the config, else 6), so a due push writes the slice and the prompt. A host finds the pending runs with `owo sync due`; with `auto_ingest` the proxy runs them itself, see [[decisions#ACP proxy runs the ingest in the background]].

Turn ids: an assistant turn's id is its `messageId`. ACP gives `session/prompt` no message id, so the user turn's id is the first agent `messageId` of that prompt plus `.user`. A live assistant turn with no `messageId`, or a user turn with no agent `messageId`, gets an id from the clock, so it is stored once. A replayed chunk with no `messageId` is not stored, because it cannot be told from a stored one. A replayed turn with a stored `messageId` adds nothing, because `sync push` skips stored ids.

## ACP proxy runs the ingest in the background

With `auto_ingest` on (see [[formats#Config resolution]]), the proxy runs the ingest of each sync run itself. When a push reports a run due, or a run still pending, the proxy starts an ingest agent in the background, with the text of that run's ingest prompt as the last argument of the ingest command, in the root, with its stdin, stdout and stderr on the null device, so nothing of it reaches the client or the session. The prompt is the whole task: it points the agent at owo ingest and ends with the run's `owo sync done`. The proxy does not wait for the agent and does not call `sync done` itself. A start that fails, or an agent that exits with an error, is one warning line. An agent that exits, with any exit code, while its run is still pending is one warning line too, which names the session and the prompt; an ingest command that cannot edit the root or run `owo`, such as a bare `claude -p`, exits 0 this way.

A dry run (`Options::dry_run`, `owo acp --dry-run`) starts no ingest agent, because the agent edits the root.

One agent runs at a time in a proxy: while one runs, a push starts no other. The run of that push stays pending, so a later push of the session reports it and starts it then. A run whose agent ended without `sync done` stays pending too, and the next push of the session starts it again.

The command is `ingest_command` from the config. Without it, the proxy takes the ingest command from the ACP agent command (`Options::agent`), by the name of an argument without its folder and its `@version`, so a path and an `npx` package name work too. `ingest_command_for` in `acp.rs` holds the table:

| Agent command | Ingest command |
| :- | :- |
| `claude-agent-acp`, `claude-code-acp` | `claude --permission-mode acceptEdits --allowedTools Bash(owo:*) -p` |
| `codex-acp` | `codex exec --sandbox workspace-write --skip-git-repo-check` |
| `opencode` (as in `opencode acp`) | `opencode run` |

The flags give the agent only what an ingest needs without a person to answer prompts: edits in the root, where it runs, and `owo` commands. With `auto_ingest` on, an agent command that is not in the table and no `ingest_command` is a config error (`Fault::NoIngestCommand`, exit 3) before the agent starts.

## Writes are all or nothing

Every file write goes through `write::Writes`: a temporary file `<name>.tmp` next to the file, synced, then a rename. The temporary file is made new, so an existing `<name>.tmp` or a symlink of that name fails the write and stays; it takes the mode of the file it replaces. A file the caller cannot write is refused before any write, and a symlink whose file is missing is an error, so no rename replaces the link. A command that writes several files builds one `Writes` and commits it once; the commit writes every temporary file before the first rename, and on an error it removes the temporary files, puts back the files it replaced, and removes the files and folders it made. `index` writes its stale files as one set; `stamp` its touched entries, the three `state.json` files and `persona/soul.json`; `sync start` the slice, the prompt and the sync cursor; `sync push` the stored turns with those; `export soulspec` the whole package; `new` its template files and folders, then its indexes, then its state files, and it removes the identity folder when a later step fails. `owo add project-readme` makes the project readme and its `notes/` folder together. The sync lock removes the `.owo/sessions` folders it made when they are empty again, so a failed sync step leaves none. Migrate writes each file this way and keeps its own cleanup. See [ADR-0006](../docs/adr/0006-every-write-is-all-or-nothing.md).

## Dry run keeps writes in memory

A dry run ([[cli#Dry run]]) is a switch in the write layer, not a parameter of each command. The function `write::dry_run` runs a call with an empty list of kept targets for its thread. While the list is there, `Writes::commit` adds its targets to the list and writes nothing. The reads `write::read`, `write::read_to_string` and `write::exists` look in the list first, and `Identity::load` adds the kept `.md` files under its folder to the files it walks, also when the folder is not on disk. So `new`, which loads the identity it just wrote to build its indexes and state files, and `migrate`, which reads back the files of `new` and checks which targets exist, run to the end as in the real run. See [ADR-0007](../docs/adr/0007-a-dry-run-keeps-writes-in-memory.md).

Three steps write outside `Writes`, and each checks `write::dry` instead. In a dry run, `new` makes no identity folder and only checks that nothing is at its path, the sync lock is not taken, and the cleanup of `new` and `migrate` after an error does not run. The list goes away when the call returns or panics, so a panic in an MCP tool call leaves no dry run on the thread.

At the end, `write::dry_run` turns the kept targets into the list of paths. For each target in commit order, it adds the folders that are not on disk, top first, then the file. It uses `Target::path`, the path the write would use, not the shown path of the error text. With `acp::Options::dry_run`, the ACP proxy runs the whole loop of its push thread inside one `write::dry_run`. Each push then reads the turns file and the sync cursor that the earlier pushes kept, and `sync push` reads its turns file with `write::read`. When the loop ends, the proxy writes the paths as lines to its warnings writer.
