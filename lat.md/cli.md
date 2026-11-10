# CLI

Every command accepts `--root`, `-i/--identity`, `--project`, `--json` and `--quiet`.

## Output

With `--json` every command prints `{ "ok", "data", "warnings", "errors" }`; otherwise readable text.

Exit codes: 0 success, 1 problems found (lint errors, exists, unknown template, stale check), 2 usage error (clap, including a date that is not YYYY-MM-DD), 3 config error (no identity selected, identity folder missing, project that is not a slug or has no `project.md`), 4 I/O error. A closed stdout (`owo search x | head`) ends the process quietly.

## Commands

Each command with its flags and exit behavior.

- `owo new <name> [--slug]` creates an identity from templates, writes indexes and state files. Exits 1 when the folder exists.
- `owo add <type> <title> [--date] [--parent] [--alias]... [--kb]` creates one entry at its deterministic path. Uses `--project` for notes; `--parent` is only for location-detail and `--kb` only for project. A second `add log` for the same day succeeds and prints `exists <path>`; for other types an existing path exits 1 with an `exists` error.
- `owo write <path> [--content <text>]` replaces the full text of one authored file under soul/, life/ or work/, from `--content` or stdin; it refuses generated files and paths outside the identity.
- `owo lint [--rule <code>]...` runs [[lint]] rules. Exits 1 when any error is found.
- `owo index [--check]` writes generated files, or reports stale ones and exits 1.
- `owo stamp [--check]` refreshes state files from hashes; see [[decisions#Stamp touches frontmatter]].
- `owo resolve <name> [--type]` prints the path a name resolves to (a file path works too, absolute or relative to the working directory); on a miss it exits 1 with up to five candidates.
- `owo show <name> [--section]` prints an entry or one section at any heading level; a `kb:<p>:<path>` name prints a file from the project KB.
- `owo links <name> [--in] [--out]` lists backlinks and outlinks; `owo links --orphans` lists unlinked entries.
- `owo context [--voice] [--look] [--with <name>]... [--notes N] [--format paths|text] [--no-touch]` prints the load set; see [[cli#Context]].
- `owo state [--days N]` prints the derived Now block, plus sessions and recent notes with `--project`; `owo recent [--days N]` lists recently stamped files.
- `owo search <query> [--scope|--in soul|life|work] [--type <t>]... [--since <date>] [--limit N]` ranks entries with BM25; no hits gives `ok: false` and exit 1. See [[decisions#Search is in-crate BM25]].
- `owo review` prints the review packet; see [[cli#Review]]. `owo review done [--through <date>]` records a finished review: `last_reviewed`, `reviewed_through` (default: newest log, event or journal date) and the MEMORY.md hash.
- `owo soul note <file> --change <text> --evidence <name>...` appends to `soul/state.json` `history`. Evidence must resolve; fewer than two links and no defining event gives a warning. `owo soul history [file]` prints the trail.
- `owo changes [--since]` lists changes by day; see [[decisions#Changes read Git, else stamps]]. `owo lint --changed` keeps diagnostics for changed files only.
- `owo export card [--spec v3|v2] [--scenario <name>] [--png <avatar> --out <file>]`, `owo export soulspec --out <dir>`, `owo export skill --out <dir>`; see [[cli#Export]].
- `owo sync start`, `owo sync push` and `owo sync done`; see [[cli#Sync]].
- `owo migrate v1 <path> --out <root>/<slug>`; see [[decisions#Migrate is mechanical]].
- `owo mcp [--remote [ADDR]] [--port N]` serves every command as an MCP tool (subcommands as `review_done`, `export_card` and so on). Without `--remote` it speaks over stdio. With `--remote` it listens on ADDR (default `127.0.0.1`), port N or 4224 (else the next free port), prints `http://<addr>:<port>/mcp`, and answers each POST with JSON. `--root`, `-i` and `--project` given to `owo mcp` are the defaults; each tool also takes `identity` and `project`. Paths in tool arguments are paths on the server.
- `owo list` prints slug, name, last activity and projects per identity. Also `owo slug <text>`, `owo templates [list | show <name>]` (list marks overrides), `owo config`.

## Context

`owo context` lists files by tier and adds a generated Now block derived from state files and frontmatter dates.

- core, always: `soul/IDENTITY.md`, `soul/SOUL.md`, `soul/MEMORY.md`, then the Now block (last activity per scope, last project, recent days, recent events, open threads and conflicts, review status, where to look next).
- voice and look: `soul/VOICE.md` and `soul/LOOK.md` with `--voice` and `--look`.
- project, with `--project <p>`: `work/README.md`, `project.md`, the newest N notes, and the KB pointer.

The KB path comes from `project.md` `kb`, else `identity.json` `projects.<p>.kb` (both relative to the identity folder, with `~/` as the home folder), else `.owo.toml` `kb` (relative to that file, with `~/` as the home folder). See [[decisions#KB pointer is always emitted]]. A project context records a session in `work/state.json` unless `--no-touch`.


## Review

`owo review` gathers what the review skill needs to compile MEMORY, as text or `--json`.

- current MEMORY.md, its word count, and whether it changed outside a review
- files whose stamp moved after `last_reviewed` (MEMORY.md excluded), with type, summary, word count, and Git `+added -removed` lines when the root is a repo
- log days after `reviewed_through`, with their bodies
- events and journal entries dated after `reviewed_through`, lessons among the changed files
- open threads and conflicts, three-word phrases on three or more unreviewed days that start and end with a content word, the last soul change

## Export

`owo export card` writes Character Card V3 JSON (V2 with `--spec v2`); `--project <p>` scopes the work side.

| Card field | Source |
| :- | :- |
| `description` | IDENTITY.md body, LOOK.md under `## Look` with its headings one level down |
| `system_prompt` | IDENTITY "Who she/he/they is/are" section, then VOICE.md |
| `personality` | SOUL.md body |
| `scenario` | with `--project`: project.md summary and body |
| `first_mes`, `alternate_greetings` | `soul/examples/greetings.md`, one per `##`; text before the first `##` is left out |
| `mes_example` | `soul/examples/good.md`, prefixed `<START>` |
| `tags`, `creator`, `character_version` | `soul/soul.json` |
| `character_book` | MEMORY.md as a constant entry, then characters, locations, cultures, factions, eras, events, lessons, and the scoped project; keys are title plus aliases; content is the summary plus `## Key facts`, or the body without `## Identifiers` when an entry has no key facts |

V3 adds `nickname` (`identity.json` `nickname`), `assets` (default icon), `creation_date`, `modification_date`, `source`, `group_only_greetings`, `creator_notes_multilingual`.

`owo export soulspec` writes `soul.json`, `SOUL.md`, `IDENTITY.md` (LOOK appended), `STYLE.md` (from VOICE), `MEMORY.md` and `examples/`, and warns when the license is outside the allowlist in `export.rs`.

`--scenario <name>` sets `scenario` from that entry's summary. `--png` writes the avatar with the card in `tEXt` chunks: `chara` (V2 JSON, base64) always and `ccv3` (V3) for V3 cards, replacing earlier ones.

`owo export skill --out <dir>` writes a `SKILL.md` that loads the identity with `owo context` and lists the read and write commands.

## Sync

Host-agnostic session sync: the host calls `sync start` after each turn and `sync done` after a successful ingest run.

`owo sync start --session <id> --transcript <path> [--format jsonl|claude-jsonl|opencode-json] [--every N] [--now]` parses user and assistant text, reads the bookmark in `<root>/.owo/sessions/<id>.json`, and counts new user turns. A run is due at `--every` turns (default `identity.json` `sync_every`, else 6), or at any new turn with `--now`.

When due it writes the slice, with two earlier user turns as overlap, to `<root>/sources/<slug>/<date>_<session>_<from>-<until>.md`, and the ingest prompt to `.owo/sessions/<id>.prompt.md`. `owo sync done --session <id> --until <n>` moves the bookmark forward; `--until` may not pass the message count the last `sync start` read. The prompt names the absolute root and runs `context --no-touch`, so ingest runs do not count as project sessions. `--project` must name an existing project. A file with no user or assistant messages for the chosen format fails. While a run is pending, a not-due `sync start` names its prompt.

`owo sync push --session <id> [--turns <json>] [--every N] [--now]` is for hosts with no transcript on this machine, such as a hosted chat over MCP. It takes a JSON array of `{id, role, text}` turns (stdin when `--turns` is left out), appends those whose id is not stored yet to `.owo/sessions/<id>.turns.jsonl`, then runs the `sync start` logic on that file. One bad turn fails the push and stores nothing; a session that syncs a local file refuses pushes. The reply adds `added` and, when due, `source_text` and `prompt_text`.

The readers keep each message's id (`id` in jsonl, `uuid` in claude-jsonl, `id` in opencode-json), and slice headings show it after the message number.
