# Formats

The on-disk formats the code reads and writes.

## Root layout

The root holds identity folders and shared working data. It works well as a Git repository; `owo changes` reads its history.

- `<slug>/`: one identity per folder.
- `sources/<slug>/`: raw source material (transcripts, notes, sync slices), outside every identity. Entries cite files here.
- `.owo/sessions/`: sync bookmarks, prompts, locks and pushed turns (`<session>.turns.jsonl`).
- `.templates/`: optional template overrides; see [[formats#Templates]].

## Identity layout

An identity is `<root>/<slug>/` with `identity.json`, `soul/`, `life/` and `work/`; `owo new` creates every folder and file listed here.

- `soul/`: `soul.json` (SoulSpec v0.5), `state.json`, `IDENTITY.md`, `SOUL.md`, `VOICE.md`, `LOOK.md`, `MEMORY.md`, `examples/{good,bad,greetings}.md`.
- `life/`: `state.json`, `_index.md`, `timeline.md`, and one folder per category: characters, locations, cultures, factions, events, eras, threads, conflicts, lessons, writing, log, journal.
- `work/`: `state.json`, `README.md`, `_index.md`, `projects/<p>/project.md`, `projects/<p>/notes/YYYY-MM-DD_<slug>.md`.

Each top-level location is a folder with `overview.md`. Rooms and angles are sibling files beside it, and location folders sit one level deep.

## Soul files

Soul Markdown files are plain Markdown with no frontmatter; their stamps live in `soul/state.json`. They are always loaded or loaded by tier, so they hold durable abstractions; scenes, quotes and dated happenings live in `life/`.

- `IDENTITY.md`: third person. Who the identity is, biography, roles, the frame of its world. It names the identity's accounts by kind only.
- `SOUL.md`: first person. Beliefs, wants, emotional logic, boundaries.
- `VOICE.md`: how the identity talks and writes, per medium.
- `LOOK.md`: durable appearance, style, signature moves.
- `MEMORY.md`: compiled long-term memory with sections Patterns, Defining moments, Lessons and Hints. Defining moments and lessons are links; hints point into `life/` and `work/`. Only review writes it.
- `examples/`: `good.md`, `bad.md`, and `greetings.md` with one `##` per greeting. Exports read them.

## Slugs and paths

One slug function turns a name into a path segment: transliterate to ASCII, lowercase, drop apostrophes, collapse every other run of non-alphanumeric characters to one hyphen, trim hyphens.

`Jay's Penthouse` becomes `jays-penthouse`. On a collision, add a short disambiguator to the slug and keep the short name as an alias. Logs are `YYYY-MM-DD.md`; journal, writing and project notes are `YYYY-MM-DD_<slug>.md`. A name gives a slug and a slug gives a path, so agents can predict a path and fall back to `owo resolve`.

## Links

Links are Obsidian-style wiki-links: `[[Title]]`, `[[Title|shown text]]`, `[[Title#Section]]`. Resolution is in [[architecture#Link resolution]].

Soul files link by name: `[[IDENTITY]]`, `[[SOUL]]`, `[[VOICE]]`, `[[LOOK]]`, `[[MEMORY]]`. Links into a project KB are `[[kb:<project>:<path>#<Section>]]`, resolved against the KB path in [[cli#Context]].

## Entry frontmatter

Every life and work entry has YAML frontmatter with `type`, `title`, `aliases`, `summary`, `status`, `created`, `updated`, `sources`. Lists are block lists (`  - item`); an empty list is `[]`.

`summary` is one line; indexes, search, the Now block and card lorebooks use it. Citations live only in frontmatter: `sources` (paths relative to `sources/`, optional `#L<a>-L<b>`, or plain text), lesson `evidence` and thread `from` are links. Bodies have no Sources, Evidence or From section.

`type` must match the folder. `status` is `canon`, `disputed` (an open conflict links the entry) or `superseded` (with `superseded_by`). Dates are `YYYY-MM-DD` and are story time: the dates inside the identity's life, not chat timestamps. Dated types (log, journal, writing, note) carry `date`, and the file name must start with it. Events need `date`; threads need `state: open|closed`; conflicts need `state: open|resolved`; location details need `parent`; notes need `project` equal to their project folder.

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
| `lesson` | `evidence` |
| `writing`, `log`, `journal` | `date` |
| `project` | `kb`, `repo`, `started` |
| `note` | `date`, `project` |

## Entry bodies

Bodies use the sections their template gives. Life entries are written in the identity's own voice, first person.

Most entries use `## Description`, `## Key facts` and `## Relationships`. Events use `## What happened`, `## Key facts` and `## Lines` (exact quotes worth keeping, linked as `[[Event#Lines]]`). Logs are bullet lists. Journal, writing and `work/README.md` are free prose.

A character's exact personal identifiers (birth date, handles, emails, phone numbers, addresses, full names of private people) go under `## Identifiers` in that character's file. Every other file refers to them by role and link; the `identifiers` rule in [[lint]] enforces this.

## identity.json

The tool manifest, read by the CLI.

- `schema` (`owo/1`), `slug`, `name`, and optional `nickname` (card export).
- `accounts`: the identity's own accounts as `{kind, handle}`. Handles appear only here.
- `budgets`: soul file word budgets that override the defaults in [[lint]].
- `review_every_days` (default 7) and `sync_every` (default 6).

`soul/soul.json` is the SoulSpec v0.5 manifest. `owo stamp` bumps its minor version when a soul file changes.

## State files

Each `state.json` has `files`: a map from scope-relative path to `{updated, hash}`. Stamps are RFC 3339 UTC and move only when the hash changes. Hashes are `b3:<blake3 hex>` of the file text.

`soul/state.json` adds `history` and `review`. Each history record is `{at, file, change, evidence}`, appended by `owo soul note`. The `review` object holds `last_reviewed`, `reviewed_through` and `memory_hash`; when MEMORY.md's hash differs from `memory_hash`, lint warns that MEMORY changed outside a review. `work/state.json` adds `projects`: per project `first_touched`, `last_touched` and `sessions`.

Commands write JSON files through a temporary file and a rename. A command that rewrites a state file or `soul/soul.json` exits 1 when the file exists but does not parse or is not an object, so a broken file is never replaced with an empty one.

## Generated files

Generated files start with a marker comment and depend only on entry content, so regeneration is deterministic and comparable. Only `owo index` writes them.

Each life category `_index.md` lists `- [[Title]] (marks): summary (aliases: ...)`, where the marks name a `state` other than open (closed, resolved) and a `status` other than canon; location details nest under their overview. `life/_index.md` lists categories with counts. `life/timeline.md` groups events under eras sorted by `date`; `era` resolves like any link (title, alias, slug, `|label`). `work/_index.md` lists the profile and projects with note counts.

## Config resolution

Root resolves from `--root`, `OWO_ROOT`, `root` in the nearest `.owo.toml`, `root` in `$XDG_CONFIG_HOME/owo/config.toml`, then `$XDG_DATA_HOME/owo`.

When the root comes from a `.owo.toml` in a parent of the working folder, owo prints a warning on stderr that names that file, since a cloned repo can ship one.

Identity resolves from `-i`, `OWO_IDENTITY`, `.owo.toml`, user config, then the only identity in the root. Project resolves from `--project`, `OWO_PROJECT`, `.owo.toml`; see [[decisions#Working project versus project filter]]. A relative `root` value resolves against the config file's folder; `~/` expands to `$HOME`.

## Templates

Templates are embedded in the binary. `<root>/.templates/<name>` overrides one, where `<name>` is the identity-relative path for identity files (`soul/IDENTITY.md`, `work/README.md`) and `<type>.md` for entries (`character.md`).

Placeholders are `{{name}}`, `{{slug}}`, `{{title}}`, `{{today}}`, `{{date}}`, `{{parent}}`, `{{project}}`. A `{{key_yaml}}` form escapes double quotes for YAML strings, and a `{{key_json}}` form escapes for JSON strings. An era's `start` and `end` are empty until the author sets them.
