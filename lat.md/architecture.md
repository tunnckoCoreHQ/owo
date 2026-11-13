# Architecture

owo is one Rust crate with a library and the `owo` binary. Each command loads an identity folder into memory, works on files only, and returns text or a JSON envelope; see [ADR-0004](../docs/adr/0004-library-core-thin-adapters.md).

## Modules

Each module owns one job. The library (`lib.rs`) holds every module but `mcp.rs` and `acp.rs`, and its public interface is small: `owo::run`, `Env`, `Output`, `Identity`, and `Cli` with its commands.

- `lib.rs` has `owo::run(&cli, env) -> Output`. It dispatches each command and collects its result through one `Out::emit` path. It prints nothing, reads no environment variables and does not exit. `Env` carries the `OWO_ROOT`, `OWO_IDENTITY`, `OWO_PROJECT`, `HOME` and XDG values, the working folder, and the stdin text for `owo write` and `owo sync push`.
- `main.rs` is the binary. It parses arguments, builds `Env` from the process with `env` (stdin read only for a command that takes it), calls `owo::run`, and prints the two strings `render` makes from the `Output`. Stderr gets the warnings as `owo: warning: <text>`, then a usage error as clap words it or the error line `owo: <error>`; stdout gets the JSON envelope with `--json`, else the text. It runs `owo mcp` and `owo acp` itself, and it keeps the SIGPIPE handling. `global_args` and `run_argv` are the one way `mcp.rs` and `acp.rs` build a command line with the global flags and run it in process.

- `cli.rs` holds the clap definitions: every command, flag and help line.

- `config.rs` resolves root, identity, project and KB from the flags and `Env`, and checks the selected identity and project (exit 3). See [[formats#Config resolution]].
- `identity.rs` walks one identity folder and parses every Markdown file into an `Entry` with a `Kind` decided by its path. `Kind::name` is the one name of a kind in output and `--type` values, and `Entry::label` gives the title, else the stem key.
- `markdown.rs` splits frontmatter and parses it with `serde-saphyr` into `serde_json` values, reads sections, extracts wiki-links outside code, counts words.
- `slug.rs` holds the one slug function.
- `templates.rs` embeds every template with `include_str!` and fills `{{key}}` placeholders.
- `create.rs` implements `owo new`; `add.rs` implements `owo add` and `owo write`.
- `index.rs` generates `_index.md` files and `life/timeline.md`.
- `state.rs` and `stamp.rs` hash files and maintain the three `state.json` files; `state::parse_day` is the one strict `YYYY-MM-DD` parser. Readers go through `state::doc` for a whole file, and `state::updated`, `state::stamps`, `state::history` and `state::review` for its parts, over the copies `Identity::load` reads; writers read, change and write a fresh copy.
- `lint.rs` runs the rules in [[lint]]; each rule appends findings and the runner names them after the rule.
- `find.rs` resolves names, shows sections, and lists backlinks, outlinks and orphans. `find::show` picks a `kb:` link or an entry for `owo show`, and `find::section_text` writes the one "no section" error for both. `find::unresolved` writes the one "does not resolve" error for `owo resolve` and every `find::one` caller (`show`, `links`, `context --with`, soul-note evidence, `export --scenario`); lint words its own link errors.
- `kb.rs` finds a project KB (`~/` is `Identity::home`, which `owo::run` sets from `Env`), builds the pointer `owo context` prints, and resolves `[[kb:...]]` links for `owo show` and lint.
- `status.rs` derives current state and the Now block; `context.rs` builds the tiered load set.
- `search.rs` is the BM25 ranker; `review.rs` builds the review packet, records finished reviews, and keeps the soul history.
- `git.rs` wraps the `git` binary; `changes.rs` implements `owo changes` and the changed-file set for `lint --changed`.
- `export.rs` writes character cards (JSON or PNG), SoulSpec packages and skill wrappers.
- `sync.rs` parses transcripts and writes session slices; `migrate.rs` converts v1 identities.
- `output.rs` defines diagnostics, the envelope, exit codes, `Out`, which collects the envelope or the text, and `Output`: the envelope, the text, the warnings, the error and the exit code. A `clap::Error` in `Output` is a usage error (exit 2), which the binary prints as clap does.
- `mcp.rs` is a module of the binary. It serves the MCP tools over stdio or HTTP, built from the clap definitions with only the public library interface. Each tool call parses its argv with clap, calls `owo::run` in process with the `env` and `render` of `main.rs`, and turns a panic into an `isError` result; see [[decisions#MCP calls the library]].
- `acp.rs` is a module of the binary. It runs an ACP agent as a child process and copies lines both ways, changes only the session requests it injects into, and stores turns through `run_argv` with `sync push`, on the public library interface only; see [[decisions#ACP proxy passes lines through]].

## Loading an identity

`Identity::load` reads `identity.json` and the three state files, then walks the folder in file-name order and parses each `.md` file once.

Path decides the kind: `life/events/x.md` is an event, `life/locations/<slug>/overview.md` a location, any other file in that folder a location detail. The files `owo index` writes (`life/_index.md`, each life category `_index.md`, `work/_index.md` and `life/timeline.md`) are generated. Files outside the layout, an `_index.md` elsewhere included, get `Kind::Unknown`, which lint reports.

`classify` in `identity.rs` maps a path to its kind, and `Kind::rel` beside it maps a kind back to the path `owo add` writes. A unit test checks that `classify` returns the same kind for the `Kind::rel` path of every type `owo add` creates.

## Link resolution

Links resolve through one lowercase key map, built once in `Identity::load`, from titles, aliases, file stems (folder name for `overview.md`), identity-relative paths and soul file names.

`resolve_link` tries the raw lowercase target, then its slug. Generated files are left out of the map, so links always land on authored entries. Only `find::resolve`, for a name typed on the command line, also accepts a file path, absolute or relative to the working directory. Links in files resolve through the key map alone, so lint gives the same result from any folder.
