# CLI

Every command accepts `--root`, `-i/--identity`, `--project`, `--json` and `--quiet`.

## Output

With `--json` every command prints `{ "ok", "data", "warnings", "errors" }`; otherwise readable text, with each warning on stderr as `owo: warning: <text>`.

With `--quiet` commands drop their confirmation lines: the path `add` and `write` print, `stamp`, `index`, `lint` and `sync` summaries, `wrote <file>`. Output that is the result itself (`show`, `search`, `context` and the like), warnings and problems still print. Lint also drops its warning diagnostics.

Exit codes: 0 success, 1 problems found (lint errors, exists, unknown template, stale check), 2 usage error (clap, including a date that is not YYYY-MM-DD), 3 config error (no identity selected, identity folder missing, project that is not a slug or has no `project.md`), 4 I/O error. A closed stdout (`owo search x | head`) ends the process quietly, except in `owo mcp` and `owo acp`, where a closed pipe is a write error.

## Commands

Each command with its flags and exit behavior.

- `owo new <name> [--slug]` creates an identity from templates, writes indexes and state files. Exits 1 when anything, even a broken symlink, is at the identity path, and leaves it alone. When a later write fails, it removes the folder it created and nothing else.
- `owo add <type> <title> [--date] [--parent] [--alias]... [--kb]` creates one entry at its deterministic path; `--date` and the `created` and `updated` fields default to the local day. Uses `--project` for notes; `--parent` is only for location-detail and `--kb` only for project. A second `add log` for the same day succeeds and prints `exists <path>`; for other types an existing path exits 1 with an `exists` error.
- `owo write <path> [--content <text>]` replaces the full text of one authored file under soul/, life/ or work/, from `--content` or stdin; it refuses empty text, generated files (after it drops `.` parts of the path) and paths outside the identity.
- `owo lint [--rule <code>]...` runs [[lint]] rules. Exits 1 when any error is found.
- `owo index [--check]` writes generated files, or reports stale ones and exits 1.
- `owo stamp [--check]` refreshes state files from hashes; see [[decisions#Stamp touches frontmatter]].
- `owo resolve <name> [--type]` prints the path a name resolves to (a file path works too, absolute or relative to the working directory); on a miss it exits 1 with up to five candidates. The error is `"<name>" does not resolve to one entry` with the candidates on the lines below; `show`, `links` and the other commands that take a name use the same sentence with `; candidates: <paths>` appended. The `--type` value is a type name such as `character` or `soul`; an unknown one is a usage error (exit 2), here and in `search`.
- `owo show <name> [--section]` prints an entry or one section at any heading level; a `kb:<p>:<path>` name prints a file from the project KB.
- `owo links <name> [--in] [--out]` lists backlinks and outlinks; `owo links --orphans` lists unlinked entries.
- `owo context [--voice] [--look] [--with <name>]... [--notes N] [--format paths]` prints the load set as text, or the file list with `--format paths`; see [[cli#Context]].
- `owo state [--days N]` prints the derived Now block, plus the last update and recent notes with `--project`.
- `owo search <query> [--scope soul|life|work] [--type <t>]... [--since <date>] [--limit N]` ranks entries with BM25; no hits gives `ok: false` and exit 1. See [[decisions#Search is in-crate BM25]].
- `owo review` prints the review packet; see [[cli#Review]]. `owo review done [--through <date>]` records a finished review: `last_reviewed`, `reviewed_through` (default: newest date of a non-blank log, an event or a journal entry) and the MEMORY.md hash.
- `owo soul note <file> --change <text> --evidence <name>...` appends to `soul/state.json` `history`. Evidence must resolve; a `#Section` or `|label` part is kept in the record and ignored for resolution; fewer than two links and no defining event gives a warning. `owo soul history [file]` prints the trail; its text shows the string evidence of an array that also holds other values. A record written by hand comes back as written, also one with a field of the wrong type, so the last record is always the newest one.
- `owo changes [--since | --days N]` lists changes by day; `--days N` starts N days before the local day; see [[decisions#Changes read Git, else stamps]]. `owo lint --changed` keeps diagnostics for changed files only.
- `owo export card [--spec v3|v2] [--scenario <name>] [--png <avatar> --out <file>]`, `owo export soulspec --out <dir>`, `owo export skill --out <dir>`; see [[cli#Export]].
- `owo sync start`, `owo sync push` and `owo sync done`; see [[cli#Sync]].
- `owo migrate v1 <path> --out <root>/<slug>`; see [[decisions#Migrate is mechanical]].
- `owo mcp [--remote [ADDR]] [--port N]` serves every command as an MCP tool (subcommands as `review_done`, `export_card` and so on). Without `--remote` it speaks over stdio. With `--remote` it listens on ADDR (default `127.0.0.1`), port N or 4224 (else the next free port), prints `http://<addr>:<port>/mcp`, and answers each POST with JSON. `--root`, `-i` and `--project` given to `owo mcp` are the defaults; each tool also takes `identity` and `project`. Paths in tool arguments are paths on the server.
- `owo acp [--inject] -- <agent command>` runs an ACP agent behind a pass-through proxy that stores the turns and gives the agent the identity; see [[decisions#ACP proxy passes lines through]]. Each session starts with the selected identity, or none, and the client switches it with the `identity` session config option or the `/owo <identity>` command. By default the proxy adds the owo MCP server to each session and tells the agent to load the identity with the `context` tool. With `--inject` it puts the output of `owo context` in the first prompt of a session and in the next prompt after a switch instead, and adds no MCP server; with no identity in the root that is a usage error (exit 2). It exits with the agent's exit code, or 1 when a signal killed the agent. It is not an MCP tool.
- `owo list` prints slug, name, last activity and projects per identity. Also `owo slug <text>` (exits 1 with a reason when the text has no letters or digits), `owo templates [list | show <name>]` (list marks overrides), `owo config`.

## Context

`owo context` lists files by tier and adds a generated Now block derived from state files and frontmatter dates.

- core, always: `soul/IDENTITY.md`, `soul/SOUL.md`, `soul/MEMORY.md`, then the Now block (last activity per scope, last project, recent days, recent events (counted back from the newest non-blank log or event date), open threads and conflicts, review status, where to look next).
- voice and look: `soul/VOICE.md` and `soul/LOOK.md` with `--voice` and `--look`.
- project, with `--project <p>`: `work/README.md`, `project.md`, the newest N notes, and the KB pointer.
- with, for each `--with <name>`: the entry it resolves to, unless the load set already holds that file.

Both formats put the Now block right after the core files.

The KB path comes from `project.md` `kb`, relative to the identity folder, with `~/` as the home folder. See [[decisions#KB pointer is always emitted]]. `owo context` writes nothing; the last project is the one with the newest file stamp under `work/projects/`.


## Review

`owo review` gathers what the review skill needs to compile MEMORY, as text or `--json`.

- current MEMORY.md, its word count, and whether it changed outside a review
- files whose stamp moved after `last_reviewed` (MEMORY.md excluded), with type, summary, word count, and Git `+added -removed` lines when the root is a repo
- log days after `reviewed_through`, with their bodies
- events and journal entries dated after `reviewed_through`, lessons among the changed files
- open threads and conflicts, three-word phrases, each inside one line, on three or more unreviewed days that start and end with a content word, the last soul change

## Export

`owo export card` writes Character Card V3 JSON (V2 with `--spec v2`); `--project <p>` scopes the work side.

| Card field | Source |
| :- | :- |
| `description` | IDENTITY.md body, LOOK.md under `## Look` with its headings one level down |
| `system_prompt` | IDENTITY "Who she/he/they is/are" section, then VOICE.md |
| `personality` | SOUL.md body |
| `scenario` | with `--project`: project.md summary and body |
| `first_mes`, `alternate_greetings` | `soul/examples/greetings.md`, one per `##` (headings inside fenced code do not count); text before the first `##` is left out |
| `mes_example` | `soul/examples/good.md`, prefixed `<START>` |
| `tags`, `creator`, `character_version` | `soul/soul.json` |
| `character_book` | MEMORY.md as a constant entry, then characters, locations, cultures, factions, eras, events, lessons, and the scoped project; keys are title plus aliases; content is the summary plus `## Key facts`, or the body without its `# ` headings and without an `Identifiers` or `Sources` section at any level (headings inside fenced code do not count) when an entry has no key facts or an empty `## Key facts` |

V3 adds `nickname` (`identity.json` `nickname`), `assets` (default icon), `creation_date`, `modification_date`, `source`, `group_only_greetings`, `creator_notes_multilingual`.

The command builds the card once, warns when `first_mes` or `mes_example` is empty, and writes that same card. Only a V3 PNG builds a second card, the V2 one for `chara`.

`owo export soulspec` writes `soul.json`, `SOUL.md`, `IDENTITY.md` (LOOK appended), `STYLE.md` (from VOICE), `MEMORY.md` and `examples/`, and warns when the license is outside the allowlist in `export.rs`.

`--scenario <name>` sets `scenario` from that entry's summary. `--png` writes the avatar with the card in `tEXt` chunks: `chara` (V2 JSON, base64) always and `ccv3` (V3) for V3 cards, replacing earlier ones.

`owo export skill --out <dir>` writes a `SKILL.md` that loads the identity with `owo context` and lists the read and write commands.

## Sync

Host-agnostic session sync: the host calls `sync start` after each turn and `sync done` after a successful ingest run.

`owo sync start --session <session_id> --transcript <path> [--format jsonl|claude-jsonl|opencode-json] [--every N] [--now]` parses user and assistant text, reads the bookmark in `<root>/.owo/sessions/<session_id>.json`, and counts new user turns. A run is due at `--every` turns (default 6), or at any new turn with `--now`.

When due it writes the slice, with two earlier user turns as overlap, to `<root>/sources/<slug>/<date>_<session_id>_<from>-<until>.md`, and the ingest prompt to `.owo/sessions/<session_id>.prompt.md`. `owo sync done --session <session_id> --until <n>` moves the bookmark forward; `--until` may not pass the message count the last `sync start` read. The prompt names the absolute root. `--project` must name an existing project. A file with no user or assistant messages for the chosen format fails. While a run is pending, a not-due `sync start` names its prompt.

`owo sync push --session <session_id> [--turns <json>] [--every N] [--now]` is for hosts with no transcript on this machine, such as a hosted chat over MCP. It takes a JSON array of `{id, role, text}` turns (stdin when `--turns` is left out), adds those whose turn id is not stored yet to `.owo/sessions/<session_id>.turns.jsonl`, then runs the `sync start` logic on that file. A turn id is the host's own message id: a number, or a string with no control characters. One bad turn fails the push and stores nothing. The store is rewritten through a temporary file, and a cut-off last line from an older write is dropped. A session that syncs a local file refuses pushes, and `sync start` refuses a session that stores pushed turns. The reply adds `added` and, when due, `source_text` and `prompt_text`. The MCP `sync_push` tool takes `turns` as a JSON array.

Pushed turns are stored in push order, not conversation order, and the bookmark counts by position. A turn the host sends after later turns lands at the end; a turn the host never sends between two pushes is lost with no signal.

The readers keep each message's turn id (`id` in jsonl, `uuid` in claude-jsonl, `id` in opencode-json), and slice headings show it after the message number, for example `## 380 user (3533)`.
