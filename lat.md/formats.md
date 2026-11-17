# Formats

The on-disk formats the code reads and writes.

## Owo dir layout

The owo dir holds what owo itself needs, apart from the identities. By default it also holds the identities dir.

- `config.toml`: the optional global config; see [[formats#Config resolution]].
- `templates/`: the templates dir, with optional template overrides; see [[formats#Templates]].
- `identities/`: the identities dir, with one identity per folder, `<slug>/`. It can be any other folder. It works well as a Git repository; `owo changes` reads its history.

## Identity layout

An identity is `<identities-dir>/<slug>/` with `identity.json`, `.gitignore`, `persona/`, `life/` and `work/`, which `owo new` creates, and `sources/` and `sessions/`, which the first sync run or migrate creates.

- `persona/`: `soul.json` (SoulSpec v0.5), `state.json`, `IDENTITY.md`, `SOUL.md`, `VOICE.md`, `LOOK.md`, `MEMORY.md`, `examples/{good,bad,greetings}.md`.
- `life/`: `state.json`, `_index.md`, `timeline.md`, and one folder per category: characters, locations, cultures, factions, events, eras, threads, conflicts, insights, writing, daily-log (the daily logs), journal.
- `work/`: `state.json`, `PROFILE.md` (the work profile), `_index.md`, `projects/<p>/README.md` (the project readme), `projects/<p>/notes/YYYY-MM-DD_<slug>.md` (the project notes).
- `sources/`: raw material (transcripts, notes, slices). Entries cite files here, so it is committed with the identity.
- `sessions/`: the session data of sync: sync cursors (`<session>.json`), ingest prompts (`<session>.prompt.md`), locks and turns files (`<session>.turns.jsonl`). These are working files of one machine, so `.gitignore` holds the line `sessions/`.

No file in `sources/` or `sessions/` is an entry; see [[decisions#Sources and sessions live in the identity folder]].

Each top-level location is a folder with `overview.md`. Rooms and angles are sibling files beside it, and location folders sit one level deep.

## Persona files

Persona files are plain Markdown with no frontmatter; their stamps live in `persona/state.json`. They are always loaded or loaded by tier, so they hold durable abstractions; scenes, quotes and dated happenings live in `life/`.

- `IDENTITY.md`: third person. Who the identity is, biography, roles, the frame of its world. It names the identity's accounts by kind only.
- `SOUL.md`: first person. Beliefs, wants, emotional logic, boundaries.
- `VOICE.md`: how the identity talks and writes, per medium.
- `LOOK.md`: durable appearance, style, signature moves.
- `MEMORY.md`: compiled long-term memory with sections Patterns, Defining moments, Insights and Hints. Defining moments and insights are wikilinks; hints point into `life/` and `work/`. Only a reflection writes it.
- `examples/`: `good.md`, `bad.md`, and `greetings.md` with one `##` per greeting. Exports read them.

## Slugs and paths

One slug function turns a name into a path segment: transliterate to ASCII, lowercase, drop apostrophes, collapse every other run of non-alphanumeric characters to one hyphen, trim hyphens.

`Jay's Penthouse` becomes `jays-penthouse`. On a collision, add a short disambiguator to the slug and keep the short name as an alias. Daily logs, journal, writing and project notes are `YYYY-MM-DD_<slug>.md`. Entry file names and project folder names must be slugs (the `schema` rule). A name gives a slug and a slug gives a path, so agents can predict a path and fall back to `owo resolve`.

## Wikilinks

Wikilinks are Obsidian-style links: `[[Title]]`, `[[Title|shown text]]`, `[[Title#Section]]`. Resolution is in [[architecture#Wikilink resolution]].

Wikilinks inside inline code or fenced code do not count. A fence opens with three or more backticks or tildes and closes on a run of the same character at least as long. A backtick with no closing backtick on its line is plain text.

Persona files link by name: `[[IDENTITY]]`, `[[SOUL]]`, `[[VOICE]]`, `[[LOOK]]`, `[[MEMORY]]`. Wikilinks into a project wiki are `[[wiki:<project>:<path>#<Section>]]`, resolved against the project wiki path in [[cli#Context]].

## Entry frontmatter

Every life and work entry has YAML frontmatter with `type`, `title`, `aliases`, `summary`, `status`, `created`, `updated`, `sources`. Lists are block lists (`  - item`); an empty list is `[]`. A title or alias holds no `#`, `|`, `[` or `]`, because those break a `[[link]]` to it; `owo add` refuses them and the `schema` rule reports them.

`summary` is one line; indexes, search, the current state and persona card lorebooks use it. Citations live only in frontmatter: `sources` (paths relative to the `sources/` folder of the identity, optional `#L<a>-L<b>`, or plain text), insight `evidence` and thread `from` are wikilinks. Bodies have no Sources, Evidence or From section, and hold no path of raw material; the rules of the `owo` skill say so too.

`type` must match the folder. `status` is `canon`, `disputed` (an open conflict links the entry) or `superseded` (with `superseded_by`). Dates are `YYYY-MM-DD` and are story time: the dates inside the identity's life, not chat timestamps. Dated types (daily-log, journal, writing, project-note) carry `date`, and the file name must start with it. Events need `date`; threads need `state: open|closed`; conflicts need `state: open|resolved`; location details need `parent`; project notes need `project` equal to their project folder.

Extra fields per type:

| Type | Extra fields |
| :- | :- |
| `character` | `pronouns`, `kind` (person, identity, animal, being) |
| `location` | `region` |
| `location-detail` | `parent` |
| `event` | `date`, `era`, `places`, `people`, `defining` |
| `era` | `start`, `end` |
| `thread` | `state`, `opened`, `from` |
| `conflict` | `state`, `affects` |
| `insight` | `evidence` |
| `writing`, `daily-log`, `journal` | `date` |
| `project-readme` | `wiki`, `repo`, `started` |
| `project-note` | `date`, `project` |

## Entry bodies

Bodies use the sections their template gives. Life entries are written in the identity's own voice, first person.

Most entries use `## Description`, `## Key facts` and `## Relationships`. Events use `## What happened`, `## Key facts` and `## Lines` (exact quotes worth keeping, linked as `[[Event#Lines]]`). Daily logs are bullet lists. A project readme starts with `## About`, one or two sentences that say what the project is, then `## What I do here`. Journal, writing and `work/PROFILE.md` are free prose.

A character's exact personal identifiers (birth date, handles, emails, phone numbers, addresses, full names of private people) go under `## Identifiers` in that character's file. Every other file refers to them by role and link; the `identifiers` rule in [[lint]] enforces this.

## identity.json

The tool manifest, read by the CLI.

- `schema` (`owo/1`), `slug`, `name`, and optional `nickname` (persona card export).
- `accounts`: the identity's own accounts as `{kind, handle}`. Handles appear only here.
- `budgets`: persona file word budgets that override the defaults in [[lint]].
- `reflect_every_days` (default 7).

`persona/soul.json` is the SoulSpec v0.5 manifest. `owo stamp` bumps its minor version when a persona file changes.

## State files

Each `state.json` has `files`: a map from scope-relative path to `{updated, hash}`. Stamps are RFC 3339 UTC and move only when the hash changes. Hashes are `b3:<blake3 hex>` of the file text.

`persona/state.json` adds `changelog` and `reflection`. Each persona change in `changelog` is `{at, file, change, evidence}`, appended by `owo persona change`. The `reflection` object holds `last_reflected`, `reflected_through` and `memory_hash`; when MEMORY.md's hash differs from `memory_hash`, lint warns that MEMORY changed outside a reflection.

Every file write is all or nothing: owo writes `<name>.tmp` next to the file and renames it over the file, and a failed write removes the temporary file. A command that writes several files (`stamp`, `index`, `sync start`, `sync push`, `export soulspec`, `new`) writes them as one set: when one fails, it puts back the files it replaced and removes the files and folders it made. A write follows a symlink to its file, and a device or pipe such as `/dev/stdout` is written in place. A symlink whose file is missing is an error and stays. A replaced file keeps its mode. A file in a read-only folder, or a file the caller cannot write, is not replaced, and a file the caller cannot read fails as a read. When `<name>.tmp` already exists, as a file or a symlink, the write fails and leaves it. Error text names the file, not its temporary file. See [[decisions#Writes are all or nothing]]. A command that rewrites a state file, `persona/soul.json` or a sync cursor exits 1 when the file exists but does not parse or is not an object, so a broken file is never replaced with an empty one. When a persona file changed, `owo stamp` reads `persona/soul.json` before it writes any file, so a broken one fails the stamp with nothing recorded or touched.

## Generated files

Generated files start with a marker comment and depend only on entry content, so regeneration is deterministic and comparable. Only `owo index` writes them.

Each life category `_index.md` lists `- [[Title]] (marks): summary (aliases: ...)`, where the marks name a `state` other than open (closed, resolved) and a `status` other than canon; location details nest under their overview. `life/_index.md` lists categories with counts. A category heading is its folder name in title case, so `daily-log` is `Daily Log`. `life/timeline.md` groups events under eras sorted by `date`; `era` resolves like any link (title, alias, slug, `|label`). `work/_index.md` lists the work profile and the projects with their project note counts.

## Config resolution

owo finds the owo dir first, then the global config, then the two folders the config can set. Each comes from its flag, then its environment variable, then the config key when it has one, then its default.

| What | Flag | Variable | Config key | Default |
| :- | :- | :- | :- | :- |
| Owo dir | `-D`, `--config-dir` | `OWO_DIR` | | `$XDG_CONFIG_HOME/owo`, else `~/.config/owo` |
| Global config | `-c`, `--config` | `OWO_CONFIG` | | `config.toml` in the owo dir |
| Identities dir | `-R`, `--identities-dir` | `OWO_IDENTITIES_DIR` | `identities_dir` | `identities/` in the owo dir |
| Templates dir | | `OWO_TEMPLATES_DIR` | `templates_dir` | `templates/` in the owo dir |

A relative path from a flag or a variable starts at the working folder. A relative path in the config starts at the folder of the config file. `~/` expands to `$HOME` in a variable and in the config. With no flag, no `OWO_DIR`, no `XDG_CONFIG_HOME` and no `HOME`, the owo dir cannot be found, which is a config error.

The global config is TOML. The default file may be missing, but a file named with `-c` or `OWO_CONFIG` must exist. A key that owo does not know is a config error, so a key from an older layout, such as `root`, does not pass in silence.

Identity resolves from `-i`, `OWO_IDENTITY`, `identity` in the config, then the only identity in the identities dir. Project resolves from `--project`, then `OWO_PROJECT`; see [[decisions#Working project versus project filter]]. The run size of `sync` is `sync_every` in the config (a number), else 6; an explicit `every` wins over it.

Two config keys set the auto-ingest of `owo acp`: `auto_ingest` (true or false, false by default) turns it on, and `ingest_command` (an array of strings, such as `["claude", "--permission-mode", "acceptEdits", "--allowedTools", "Bash(owo:*)", "-p"]`) is the ingest command, which gets the ingest prompt as its last argument. Without `ingest_command` the proxy takes the ingest command from the ACP agent command; see [[decisions#ACP proxy runs the ingest in the background]].

See [[decisions#One owo dir, and no local config]].

## Templates

Templates are embedded in the binary. `<templates-dir>/<name>` overrides one, where `<name>` is the identity-relative path for identity files (`persona/IDENTITY.md`, `work/PROFILE.md`, `.gitignore`) and `<type>.md` for entries (`character.md`, `project-readme.md`).

Placeholders are `{{name}}`, `{{slug}}`, `{{title}}`, `{{today}}`, `{{date}}`, `{{parent}}`, `{{project}}`. A `{{key_yaml}}` form escapes double quotes for YAML strings, and a `{{key_json}}` form escapes for JSON strings. Filling is one pass, so `{{key}}` text inside a value stays as written. An era's `start` and `end` are empty until the author sets them.
