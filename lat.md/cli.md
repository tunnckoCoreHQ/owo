# CLI

Every command accepts `-D/--config-dir`, `-c/--config`, `-R/--identities-dir`, `-i/--identity`, `--project`, `--json` and `--quiet`. The first three say where owo finds its files; see [[formats#Config resolution]].

## Output

With `--json` every command prints `{ "ok", "data", "warnings", "errors" }`, and a dry run adds `would_write`; otherwise readable text, with each warning on stderr as `owo: warning: <text>`.

With `--quiet` commands drop their confirmation lines: the path `add` and `write` print, `stamp`, `index`, `lint` and `sync` summaries, `wrote <file>`. Output that is the result itself (`show`, `search`, `context` and the like), warnings and problems still print. Lint also drops its warning diagnostics.

Exit codes: 0 success, 1 problems found (lint errors, exists, unknown template, stale check), 2 usage error (clap, including a date that is not YYYY-MM-DD), 3 config error (no identity selected, identity folder missing, project that is not a slug or has no `README.md`, `auto_ingest` with no ingest command for the agent), 4 I/O error. A closed stdout (`owo search x | head`) ends the process quietly, except in `owo mcp` and `owo acp`, where a closed pipe is a write error.

## Dry run

Each command that writes files takes `--dry-run`, and its MCP tool takes `dryRun`: `new`, `write`, `add`, `stamp`, `index`, `reflect done`, `persona change`, `export persona-card`, `export skill`, `export soulspec`, `sync start`, `sync push`, `sync done` and `migrate v1`. The command runs as usual, writes nothing, and reports what it would write. See [[decisions#Dry run keeps writes in memory]].

- The list names each file the command would write, and before it each folder the command would make for it, once each, in write order. A folder ends in `/`. A path is the one the command would write: the request path joined onto the identities dir or the working folder.
- With `--json` the envelope is the one of the real run, with `would_write` after `errors`: `{ "ok", "data", "warnings", "errors", "would_write" }`.
- The text drops the confirmation lines, as `--quiet` does, then prints `would write <path>` for each path, or `would write nothing`.

A dry run does not try the writes, so it gives the errors that come before the writes, with their exit codes, and not the errors of a write. For example, `owo new <name> --dry-run` for an identity that exists exits 1, but a file in a read-only folder is still in the list. A dry run takes no sync lock.

The ACP proxy takes `--dry-run` too: `owo acp --dry-run -- <agent command>` stores no turns and starts no ingest agent, also with `auto_ingest`. Each push sees the turns that the earlier pushes would have stored, so a run still comes due. When the agent exits, the proxy writes `owo: would write <path>` on stderr for each path its pushes would write. The MCP server that the proxy adds is a separate `owo mcp` process, and its tools write unless a call sets `dryRun`. The `owo mcp` command itself writes no files and takes no `--dry-run`.

## Commands

Each command with its flags and exit behavior.

- `owo new <name> [--slug]` creates an identity from templates, writes indexes and state files. Exits 1 when anything, even a broken symlink, is at the identity path, and leaves it alone. When a later write fails, it removes the folder it created and nothing else.
- `owo add <type> <title> [--date] [--parent] [--alias]... [--wiki]` creates one entry at its deterministic path; `--date` and the `created` and `updated` fields default to the local day. Uses `--project` for project notes; `--parent` is only for location-detail and `--wiki` only for project-readme. Each day has one daily log: `add daily-log` for a day that has one, with or without a title, succeeds and prints `exists <path>` of that log, whatever its slug; for a day without one it needs a title. For other types an existing path exits 1 with an `exists` error.
- `owo write <path> [--content <text>]` replaces the full text of one authored file under persona/, life/ or work/, from `--content` or stdin; it refuses empty text, generated files (after it drops `.` parts of the path) and paths outside the identity.
- `owo lint [--rule <code>]...` runs [[lint]] rules. Exits 1 when any error is found.
- `owo index [--check]` writes generated files, or reports stale ones and exits 1.
- `owo stamp [--check]` refreshes state files from hashes; see [[decisions#Stamp touches frontmatter]].
- `owo resolve <name> [--kind]` prints the path a name resolves to (a file path works too, absolute or relative to the working folder); on a miss it exits 1 with up to five candidates. The name is required: with no name it is a usage error (exit 2), before the identities dir and the identity resolve, so it is the same error with one identity or more. An empty or blank name has no candidates. The error is `"<name>" does not resolve to one entry` with the candidates on the lines below; `show`, `links` and the other commands that take a name use the same sentence with `; candidates: <paths>` appended. The `--kind` value is a kind name such as `character`, `persona` or `work-profile` (the kind of `work/PROFILE.md`); an unknown one is a usage error (exit 2), here and in `search`. The flag is `--kind` and not `--type`, because it takes every kind, also those with no frontmatter type; `owo add <type>` takes only the types.
- `owo show <name> [--section]` prints an entry or one section at any heading level; a `wiki:<p>:<path>` name prints a file from the project wiki.
- `owo links <name> [--in] [--out]` lists backlinks and outlinks; `owo links --orphans` lists unlinked entries. `owo wikilinks` is an alias of `owo links` in the CLI only; the MCP tool and the lint rule keep the name `links`.
- `owo context [--voice] [--look] [--with <name>]... [--notes N] [--format paths]` prints the load set as text, or the file list with `--format paths`; see [[cli#Context]].
- `owo state [--days N]` prints the derived current state, plus the last update and recent project notes with `--project`.
- `owo search <query> [--scope persona|life|work] [--kind <k>]... [--since <date>] [--limit N]` ranks entries with BM25; no hits gives `ok: false` and exit 1. See [[decisions#Search is in-crate BM25]].
- `owo reflect` prints the reflection digest; see [[cli#Reflect]]. `owo reflect done [--through <date>]` records a finished reflection: `last_reflected`, `reflected_through` (default: newest date of a non-blank daily log, an event or a journal entry) and the MEMORY.md hash.
- `owo persona [--id] [--soul] [--voice] [--look] [--memory]` prints `persona/IDENTITY.md` and `persona/SOUL.md`, or only the persona files the flags name, in that order, each after an `<!-- owo: <path> -->` line. A missing one exits 1. See [[decisions#Persona prints persona files without the load set]].
- `owo persona change <file> --change <text> --evidence <name>...` appends to `persona/state.json` `changelog`. Evidence must resolve; a `#Section` or `|label` part is kept in the record and ignored for resolution; fewer than two links and no defining event gives a warning. `owo persona changelog [file]` prints the persona changelog; its text shows the string evidence of an array that also holds other values. A persona change written by hand comes back as written, also one with a field of the wrong type, so the last persona change is always the newest one.
- `owo changes [--since | --days N]` lists changes by day; `--days N` starts N days before the local day; see [[decisions#Changes read Git, else stamps]]. `owo lint --changed` keeps diagnostics for changed files only.
- `owo export persona-card [--spec v3|v2] [--scenario <name>] [--png <avatar> --out <file>]`, `owo export soulspec --out <dir>`, `owo export skill --out <dir>`; see [[cli#Export]].
- `owo sync start`, `owo sync push`, `owo sync done` and `owo sync due`; see [[cli#Sync]].
- `owo migrate v1 <path> --out <identities-dir>/<slug>`; see [[decisions#Migrate is mechanical]].
- `owo mcp [--remote [ADDR]] [--port N]` serves every command as an MCP tool (subcommands as `reflect_done`, `export_persona_card` and so on; a `-` in a command name is `_` in its tool name). Without `--remote` it speaks over stdio. With `--remote` it listens on ADDR (default `127.0.0.1`), port N or 4224 (else the next free port), prints `http://<addr>:<port>/mcp`, and answers each POST with JSON. `-D`, `-c`, `-R`, `-i` and `--project` given to `owo mcp` are the defaults; each tool also takes `identity` and `project`. Paths in tool arguments are paths on the server.
- `owo acp [--inject] [--dry-run] -- <agent command>` runs an ACP agent behind a pass-through proxy that stores the turns and gives the agent the identity; see [[decisions#ACP proxy passes lines through]]. Each session starts with the selected identity, or none, and the client switches it with the `identity` session config option or the `/owo <identity>` command. By default the proxy adds the owo MCP server to each session and tells the agent to load the identity with the `context` tool. With `--inject` it puts the output of `owo context` in the first prompt of a session and in the next prompt after a switch instead, and adds no MCP server; with no identity in the identities dir that is a usage error (exit 2). With `auto_ingest` in the config, and not with `--dry-run`, it starts an ingest agent in the background for each due or pending run, one at a time; see [[decisions#ACP proxy runs the ingest in the background]]. It exits with the agent's exit code, or 1 when a signal killed the agent. It is not an MCP tool.
- `owo list` prints slug, name, last activity and projects per identity. Also `owo slug <text>` (exits 1 with a reason when the text has no letters or digits), `owo templates [list | show <name>]` (list marks overrides), `owo config` (the owo dir, the config file, the identities dir and the templates dir, each with where it comes from: flag, environment, global config `<file>` or default; then the identity with where it comes from, and the project), and `owo guide [embody|create|ingest|reflect|rules|glossary]` (the entry file of the `owo` skill, or one file of it; see [[decisions#Skills live in the repo]]).

## Context

`owo context` lists files by tier and adds the generated current state derived from state files and frontmatter dates.

- core, always: `persona/IDENTITY.md`, `persona/SOUL.md`, `persona/MEMORY.md`.
- voice and look: `persona/VOICE.md` and `persona/LOOK.md` with `--voice` and `--look`.
- work, with `--project <p>`: `work/PROFILE.md`, the project readme `work/projects/<p>/README.md`, the newest N project notes, and the project wiki path.
- with, for each `--with <name>`: the entry it resolves to, unless the load set already holds that file.
- state, always: the current state, under `## Current state` (last activity per scope, last project, recent days, recent events (counted back from the newest non-blank daily log or event date), open threads and conflicts, reflection status, where to look next).

Both formats put the current state last, after the project wiki path.

The project wiki path comes from `wiki` in the project readme, relative to the identity folder, with `~/` as the home folder. See [[decisions#Project wiki path is always emitted]]. `owo context` writes nothing; the last project is the one with the newest file stamp under `work/projects/`.


## Reflect

`owo reflect` prints the reflection digest: what a reflection needs to compile MEMORY, as text or `--json`.

- current MEMORY.md, its word count, and whether it changed outside a reflection
- files whose stamp moved after `last_reflected` (MEMORY.md excluded), with type, summary, word count, and Git `+added -removed` lines when the identity folder is in a repo
- daily logs after `reflected_through`, with their bodies
- events and journal entries dated after `reflected_through`, insights among the changed files
- open threads and conflicts, three-word phrases, each inside one line, on three or more of those daily logs that start and end with a content word, the last persona change

## Export

`owo export persona-card` writes the persona card as Character Card V3 JSON (V2 with `--spec v2`); `--project <p>` narrows the work scope to one project.

| Persona card field | Source |
| :- | :- |
| `description` | IDENTITY.md body, LOOK.md under `## Look` with its headings one level down |
| `system_prompt` | IDENTITY "Who she/he/they is/are" section, then VOICE.md |
| `personality` | SOUL.md body |
| `scenario` | with `--project`: the project readme summary and body |
| `first_mes`, `alternate_greetings` | `persona/examples/greetings.md`, one per `##` (headings inside fenced code do not count); text before the first `##` is left out |
| `mes_example` | `persona/examples/good.md`, prefixed `<START>` |
| `tags`, `creator`, `character_version` | `persona/soul.json` |
| `character_book` | MEMORY.md as a constant entry, then characters, locations, cultures, factions, eras, events, insights, and that project; keys are title plus aliases; content is the summary plus `## Key facts`, or the body without its `# ` headings and without an `Identifiers` or `Sources` section at any level (headings inside fenced code do not count) when an entry has no key facts or an empty `## Key facts` |

V3 adds `nickname` (`identity.json` `nickname`), `assets` (default icon), `creation_date`, `modification_date`, `source`, `group_only_greetings`, `creator_notes_multilingual`.

The command builds the card once, warns when `first_mes` or `mes_example` is empty, and writes that same card. Only a V3 PNG builds a second card, the V2 one for `chara`.

`owo export soulspec` writes `soul.json`, `SOUL.md`, `IDENTITY.md` (LOOK appended), `STYLE.md` (from VOICE), `MEMORY.md` and `examples/`, and warns when the license is outside the allowlist in `export.rs`.

`--scenario <name>` sets `scenario` from that entry's summary. `--png` writes the avatar with the card in `tEXt` chunks: `chara` (V2 JSON, base64) always and `ccv3` (V3) for V3 cards, replacing earlier ones.

`owo export skill --out <dir>` writes a `SKILL.md` that loads the identity with `owo context` and lists the read and write commands.

## Sync

Host-agnostic session sync: the host calls `sync start` after each turn, and the ingest agent calls `sync done` at the end of a successful run. The session data and the slices of an identity are in its own folder; see [[formats#Identity layout]].

`owo sync start --session <session_id> --transcript <path> [--format jsonl|claude-jsonl|opencode-json] [--every N] [--now]` parses user and assistant text, reads the sync cursor in `sessions/<session_id>.json` of the identity folder, and counts new user turns. A run is due at `--every` turns, or at any new turn with `--now`. Without `--every` the run size is `sync_every` from the config (see [[formats#Config resolution]]), else 6; the MCP `every` argument works the same way.

When due it writes the slice, with two earlier user turns as overlap, to `sources/<date>_<session_id>_<from>-<until>.md`, and the ingest prompt to `sessions/<session_id>.prompt.md`, both in the identity folder. The `slice` and `prompt` paths in the reply start at the identity folder. `owo sync done --session <session_id> --until <n>` moves the sync cursor of the selected identity forward; `--until` may not pass the turn count the last `sync start` read. `--project` must name an existing project. A file with no user or assistant turns for the chosen format fails. While a run is pending, a not-due `sync start` names its prompt.

The ingest prompt starts with "Use owo ingest", which points the agent at the ingest path of the `owo` skill, and its last step is the `owo sync done` command of the run, so the prompt is the whole task of an ingest agent. It names the absolute identity folder, and tells the agent to run every operation of the run on the interface that gave it the run. An agent that read the prompt from a file starts each owo command, the last one too, with `owo --identities-dir <dir> -i <slug>`. An agent that got the prompt in a `sync_push` result uses the tools of that same MCP server with `identity` set, and its `guide` tool for the skill. Neither uses another owo server or connector. See [[decisions#The ingest prompt names its interface]].

`owo sync push --session <session_id> [--turns <json>] [--every N] [--now]` is for a session with no transcript on the machine of owo, such as the session of a remote host over MCP. It takes a JSON array of `{id, role, text}` turns (stdin when `--turns` is left out), adds those whose turn id is not stored yet to `sessions/<session_id>.turns.jsonl`, then runs the `sync start` logic on that file. A turn id is the host's own id for that turn: a number, or a string with no control characters. One bad turn fails the push and stores nothing. The turns file is rewritten through a temporary file, and a cut-off last line from an older write is dropped. A session that syncs a local file refuses pushes, and `sync start` refuses a session that stores pushed turns. The reply adds `added`, and `slice_text` and `prompt_text` whenever the session has a pending run: in the push that makes the run, and in each later push until `sync done`. A push of no turns gives the two texts again. The MCP `sync_push` tool takes `turns` as a JSON array.

Pushed turns are stored in push order, not conversation order, and the sync cursor counts by position. A turn the host sends after later turns lands at the end; a turn the host never sends between two pushes is lost with no signal.

`owo sync due` lists the sessions that have a pending run: the identity, the session id, the prompt path from the identity folder, and the turns `from` to `until` the run covers; the text prints one line per session, `<session_id> <from>-<until> <identity>/<prompt>`. With `-i` (or the MCP `identity` argument) it reads the sync cursors of that identity; without it, those of every identity in the identities dir. An identity from the environment or the config does not narrow it; the `-i` of `owo mcp`, such as the one the ACP proxy passes, does, since it is the default `identity` of every tool. See [[decisions#Working project versus project filter]]. The MCP tool is `sync_due`.

The readers keep each turn's id (`id` in jsonl, `uuid` in claude-jsonl, `id` in opencode-json), and slice headings show it after the turn number, for example `## 380 user (3533)`.
