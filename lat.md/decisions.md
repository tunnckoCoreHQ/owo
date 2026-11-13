# Decisions

Implementation decisions and how they shape current behavior. The three ADRs in `docs/adr/` record the general decisions; a section links the ADR that covers it.

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

`owo mcp` is a newline-delimited JSON-RPC loop in `mcp.rs` with no MCP crate. Each tool call builds the matching argv, parses it with clap, and calls `owo::run` in process, so a tool and a command always behave alike. See [ADR-0003](../docs/adr/0003-library-core-thin-adapters.md) and [ADR-0002](../docs/adr/0002-small-dependency-set.md).

The tool list is read from the clap definitions (`Cli::command()`): each subcommand is a tool, each flag a parameter with its help text, type and allowed values, so a new flag needs no MCP change.

Most tools add `--json` and return the envelope; `context` and `show` return text, which is what an agent reads. Positional values go after `--` and flag values are passed as `--flag=value`, so a value never parses as a flag. A failed command sets `isError`; an unknown tool name gets a -32602 error. `initialize` answers the client's protocol version when it is one the server speaks (2025-06-18, 2025-03-26, 2024-11-05), else 2025-06-18. A line that is not UTF-8 JSON gets a -32700 parse error and the server goes on.

The `--remote` mode is a small HTTP/1.1 loop over `std::net`: one request at a time, `POST /mcp` with a JSON reply, 202 for notifications, 405 for other methods. It has no authentication, so whoever reaches the URL can read and write the identity. To keep web pages out, it answers 403 to a request with an `Origin` header and 415 to a POST without `Content-Type: application/json`, and 411 to a POST without `Content-Length` or with any `Transfer-Encoding` header. One request gets 30 seconds to arrive and its reply 30 seconds to leave; headers are capped at 16KB (431) and the body at 8MB (413). A refused body it did not read is discarded after the reply, so the client sees the whole status; the discard stops after 200ms with nothing arriving, or after 2 seconds, so an idle client cannot hold the server.
