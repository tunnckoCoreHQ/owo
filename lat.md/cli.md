# CLI

Commands implemented so far. Every command accepts `--root`, `-i/--identity`, `--project`, `--json` and `--quiet`.

## Output

With `--json` every command prints `{ "ok", "data", "warnings", "errors" }`; otherwise readable text.

Exit codes: 0 success, 1 problems found (lint errors, exists, unknown template, stale check), 2 usage error (clap), 3 config error (no identity selected), 4 I/O error.

## Commands

The implemented command set; see `docs/design.md` section 7 for the full plan.

- `owo new <name> [--slug]` creates an identity from templates, writes indexes and state files. Exits 1 when the folder exists.
- `owo add <type> <title> [--date] [--parent] [--alias]...` creates one entry at its deterministic path. Uses `--project` for notes. A second `add log` for the same day succeeds and prints `exists <path>`.
- `owo lint [--rule <code>]...` runs [[lint]] rules. Exits 1 when any error is found.
- `owo index [--check]` writes generated files, or reports stale ones and exits 1.
- `owo stamp [--check]` refreshes state files from hashes; see [[decisions#Stamp touches frontmatter]].
- `owo resolve <name> [--type]` prints the path a name resolves to; on a miss it exits 1 with up to five candidates.
- `owo show <name> [--section]` prints an entry or one `## ` section.
- `owo links <name> [--in] [--out]` lists backlinks and outlinks; `owo links --orphans` lists unlinked entries.
- `owo context [--voice] [--look] [--with <name>]... [--notes N] [--format paths|text] [--no-touch]` prints the load set; see [[cli#Context]].
- `owo state` prints the derived Now block; `owo recent [--days N]` lists recently stamped files.
- `owo search <query> [--scope soul|life|work] [--limit N]` ranks entries with BM25; see [[decisions#Search is in-crate BM25]].
- `owo review` prints the review packet; see [[cli#Review]]. `owo review done [--through <date>]` records a finished review: `last_reviewed`, `reviewed_through` (default: newest log date) and the MEMORY.md hash.
- `owo soul note <file> --change <text> --evidence <name>...` appends to `soul/state.json` `history`. Evidence must resolve; fewer than two links and no defining event gives a warning. `owo soul history [file]` prints the trail.
- `owo changes [--since]` lists changes by day; see [[decisions#Changes read Git, else stamps]]. `owo lint --changed` keeps diagnostics for changed files only.
- `owo export card [--spec v3|v2] [--out]` and `owo export soulspec --out <dir>`; see [[cli#Export]].
- `owo list`, `owo slug <text>`, `owo templates [name]`, `owo config`.

## Context

`owo context` lists files by tier and adds a generated Now block derived from state files and frontmatter dates.

- core, always: `soul/IDENTITY.md`, `soul/SOUL.md`, `soul/MEMORY.md`, then the Now block (last activity per scope, last project, recent days, recent events, open threads and conflicts, review status, where to look next).
- voice and look: `soul/VOICE.md` and `soul/LOOK.md` with `--voice` and `--look`.
- project, with `--project <p>`: `work/README.md`, `project.md`, the newest N notes, and the KB pointer.

The KB path comes from `project.md` `kb`, else `identity.json` `projects.<p>.kb` (both relative to the identity folder), else `.owo.toml` `kb` (relative to that file). See [[decisions#KB pointer is always emitted]]. A project context records a session in `work/state.json` unless `--no-touch`.


## Review

`owo review` gathers what the review skill needs to compile MEMORY, as text or `--json`.

- current MEMORY.md, its word count, and whether it changed outside a review
- files whose stamp moved after `last_reviewed` (MEMORY.md excluded), with type, summary, word count, and Git `+added -removed` lines when the root is a repo
- log days after `reviewed_through`, with their bodies
- events and journal entries dated after `reviewed_through`, lessons among the changed files
- open threads and conflicts, three-word phrases on three or more unreviewed days, the last soul change

## Export

`owo export card` writes Character Card V3 JSON (V2 with `--spec v2`); `--project <p>` scopes the work side.

| Card field | Source |
| :- | :- |
| `description` | IDENTITY.md body, LOOK.md under `## Look` |
| `system_prompt` | IDENTITY "Who she/he/they is/are" section, then VOICE.md |
| `personality` | SOUL.md body |
| `scenario` | with `--project`: project.md summary and body |
| `first_mes`, `alternate_greetings` | `soul/examples/greetings.md`, one per `##` |
| `mes_example` | `soul/examples/good.md`, prefixed `<START>` |
| `tags`, `creator`, `character_version` | `soul/soul.json` |
| `character_book` | MEMORY.md as a constant entry, then characters, locations, cultures, factions, eras, events, lessons, and the scoped project; keys are title plus aliases; `## Identifiers` and `## Sources` are left out |

V3 adds `nickname` (first IDENTITY alias), `assets` (default icon), `creation_date`, `modification_date`, `source`, `group_only_greetings`, `creator_notes_multilingual`.

`owo export soulspec` writes `soul.json`, `SOUL.md`, `IDENTITY.md` (LOOK appended), `STYLE.md` (from VOICE), `MEMORY.md` and `examples/`, and warns when the license is outside the allowlist in `export.rs`.
