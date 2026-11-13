# Architecture

owo is one Rust crate with a library and the `owo` binary. Each command loads an identity folder into memory, works on files only, and returns a typed result; the binary turns it into text or a JSON envelope. See [ADR-0005](../docs/adr/0005-typed-library-api.md).

## Modules

Each module owns one job. The library (`src/lib.rs` and the modules beside it) is the typed API. The ACP proxy (`owo::acp`) is in the library too, as a consumer of that API. The binary (`src/bin/owo/`) holds the CLI and the MCP server, the two consumers that render command output; see [ADR-0005](../docs/adr/0005-typed-library-api.md).

### Library

- `lib.rs` is the public API. `Root::open(&Select, &Env)` resolves the root, identity and project and keeps the config warnings; `Root::identity` loads the selected identity and sets its home folder from `Env`. `Root` has `list`, `create`, `templates`, `template`, `sync` (a `SessionSync` with `start` and `push`, for the checked identity and project) and `sync_done`. `Identity` has one method per identity command: `lint`, `write`, `add`, `stamp`, `index`, `stale_index`, `resolve`, `show`, `links`, `orphans`, `context`, `context_text`, `state`, `search`, `review`, `review_done`, `soul_note`, `soul_history`, `changes`, `export_card`, `export_skill`, `export_soulspec`, and `check_project`. The free functions are `slug`, `migrate_v1`, `entry_types` and `lint_rules`, and `owo::acp` is the ACP proxy. A method that takes a project checks it (`Error::Config` when it is not found). Each call returns `owo::Error` on failure: `Usage`, `Config`, `Io` or `Failed`, with the whole error text. The library parses no command line, renders no command output, has no exit codes, prints nothing, reads no environment variable and takes no stream of the process. Three things still come from the process: the time zone, which the local day uses, the working folder for a relative path in an argument such as `--out`, and the environment of the `git` child that `changes` and `lint` with `changed` run. The `Env.cwd` value is for the config only (a relative root and the `.owo.toml` search).
- `config.rs` resolves root, identity, project and KB from `Select` and `Env`, and checks the selected identity and project (`Error::Config`). See [[formats#Config resolution]].
- `identity.rs` walks one identity folder and parses every Markdown file into an `Entry` with a `Kind` decided by its path. `Kind::name` is the one name of a kind in output and `--type` values, and `Entry::label` gives the title, else the stem key.
- `markdown.rs` splits frontmatter and parses it with `serde-saphyr` into `serde_json` values, reads sections, extracts wiki-links outside code, counts words.
- `slug.rs` holds the one slug function.
- `templates.rs` embeds every template with `include_str!` and fills `{{key}}` placeholders.
- `create.rs` implements `owo new`; `add.rs` implements `owo add` and `owo write`.
- `index.rs` generates `_index.md` files and `life/timeline.md`.
- `state.rs` and `stamp.rs` hash files and maintain the three `state.json` files; `state::parse_day` is the one strict `YYYY-MM-DD` parser. Readers go through `state::doc` for a whole file, and `state::updated`, `state::stamps`, `state::history` and `state::review` for its parts, over the copies `Identity::load` reads; writers read, change and write a fresh copy.
- `lint.rs` runs the rules in [[lint]]; each rule appends findings and the runner names them after the rule.
- `find.rs` resolves names, shows sections, and lists backlinks, outlinks and orphans. `find::show` picks a `kb:` link or an entry for `owo show`, and `find::section_text` writes the one "no section" error for both. `find::unresolved` writes the one "does not resolve" error for `owo resolve` and every `find::one` caller (`show`, `links`, `context --with`, soul-note evidence, `export --scenario`); lint words its own link errors.
- `kb.rs` finds a project KB (`~/` is `Identity::home`, which `Root::identity` sets from `Env`), builds the pointer `owo context` prints, and resolves `[[kb:...]]` links for `owo show` and lint.
- `status.rs` derives current state and the Now block; `context.rs` builds the tiered load set and its text.
- `search.rs` is the BM25 ranker; `review.rs` builds the review packet, records finished reviews, and keeps the soul history.
- `git.rs` wraps the `git` binary; `changes.rs` implements `owo changes` and the changed-file set for `lint --changed`.
- `export.rs` writes character cards (JSON or PNG), SoulSpec packages and skill wrappers.
- `sync.rs` parses transcripts and writes session slices; `migrate.rs` converts v1 identities.
- `output.rs` defines `Diagnostic` and its `Level`.
- `acp.rs` is `owo::acp`, the ACP proxy, a consumer of the API above in the library, so a program that links owo can run it on streams it gives. `Proxy::new` resolves the identity and lists the identities of the root with `Root::open`, `Root::identity` and `Root::list` before the agent starts, with `Options` for the injection mode and a writer for warning lines. `Proxy::run` copies lines both ways on the four streams the caller gives it, changes only the lines that carry the injection, the identity picker and the `/owo` command, answers the picker and the command itself, reads the injected context with `Identity::context` and `context_text`, and stores turns per session identity with `SessionSync::push`; see [[decisions#ACP proxy passes lines through]].

### Binary

- `main.rs` only connects the process to the consumers: it parses arguments, builds `Env` from the environment with `env`, and passes the process stdin to `run::run`. It prints the two strings `Output::render` makes; clap itself prints a usage error, so it keeps its colour on a terminal. Stderr gets the warnings as `owo: warning: <text>`, then a usage error as clap words it or the error line `owo: <error>`; stdout gets the JSON envelope with `--json`, else the text. For `owo mcp` it takes over stdin and stdout (`own_stdio`, `detach_stdio`) and passes the copies to `mcp::serve`, or binds with `mcp::bind`, prints the URL and calls `mcp::serve_http`. For `owo acp` it builds `owo::acp::Proxy` with stderr as the warnings writer, starts the agent process, hands its pipes and the process stdin and stdout to `Proxy::run`, stops the agent when that fails, and exits with the agent's code. It keeps the SIGPIPE handling for every command but `owo mcp` and `owo acp`, which ignore the signal.
- `cli.rs` holds the clap definitions: every command, flag and help line.
- `run.rs` is the CLI: `run::run(&cli, &env, stdin) -> Output` maps each command to its library call and renders the result with `render`. The `stdin` reader gives the text that `owo write` and `owo sync push` read only when they need it, after the config resolves.
- `render.rs` turns the typed result of each command into its `Output` through one `Out::emit` path, for the CLI and the MCP server alike. `finish` keeps the warnings found before a failure, and `open` and `load` open the root and load the identity with the config warnings.
- `text.rs` builds the text of each command from its typed result: lint lines, tables, the review packet and the other reports.
- `output.rs` defines the envelope, exit codes, `Out`, which collects the envelope or the text, and `Output`: the envelope, the text, the warnings, the error and the exit code. `Output::render` turns it into the stdout text and the stderr text, and `Output::warning_lines` gives the warnings alone. The error kind gives the exit code: a `clap::Error` or `Error::Usage` 2 (an `Error::Usage` is worded as clap words its own errors), `Error::Config` 3, `Error::Io` 4, any other 1.
- `mcp.rs` is the MCP server. It builds the tools from the clap definitions. `reply` answers one JSON-RPC message, `serve` answers each line of a reader on a writer, and `serve_http` answers each POST on a listener that `bind` opens. Each tool call checks its JSON arguments, maps them to the typed library call, renders the result with `render` and `Output::render`, and turns a panic into an `isError` result; see [[decisions#MCP calls the library]].
- `tests.rs` runs the MCP server on in-memory streams.

## Loading an identity

`Identity::load` reads `identity.json` and the three state files, then walks the folder in file-name order and parses each `.md` file once.

Path decides the kind: `life/events/x.md` is an event, `life/locations/<slug>/overview.md` a location, any other file in that folder a location detail. The files `owo index` writes (`life/_index.md`, each life category `_index.md`, `work/_index.md` and `life/timeline.md`) are generated. Files outside the layout, an `_index.md` elsewhere included, get `Kind::Unknown`, which lint reports.

`classify` in `identity.rs` maps a path to its kind, and `Kind::rel` beside it maps a kind back to the path `owo add` writes. A unit test checks that `classify` returns the same kind for the `Kind::rel` path of every type `owo add` creates.

## Link resolution

Links resolve through one lowercase key map, built once in `Identity::load`, from titles, aliases, file stems (folder name for `overview.md`), identity-relative paths and soul file names.

`resolve_link` tries the raw lowercase target, then its slug. Generated files are left out of the map, so links always land on authored entries. Only `find::resolve`, for a name typed on the command line, also accepts a file path, absolute or relative to the working directory. Links in files resolve through the key map alone, so lint gives the same result from any folder.
