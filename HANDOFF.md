# Handoff

A standalone brief for the next coding agent. Read this, then `lat.md/lat.md`, then the sections of `docs/design.md` named per task.

## What owo is

owo ("one world, one identity") is a Rust CLI for living agent identities. An identity is a folder `<root>/<slug>/`:

- `identity.json`: name, accounts, budgets, `review_every_days`, `projects.<p>.kb`.
- `soul/`: IDENTITY, SOUL, VOICE, LOOK, MEMORY (Markdown), `soul.json` (SoulSpec manifest), `examples/`, `state.json` (stamps, `history`, `review`).
- `life/`: characters, locations, cultures, factions, eras, events, threads, conflicts, lessons, writing, log, journal; `timeline.md` and `_index.md` files are generated; `state.json`.
- `work/`: `README.md` (profile), `projects/<p>/project.md` and `projects/<p>/notes/<date>_<slug>.md`; `state.json` (stamps, per-project sessions).

owo does every deterministic job (paths, templates, validation, indexes, stamps, search, context, review packets, exports). Agent skills in `skills/` do the judgment work and call owo.

## Build and test

```sh
export PATH=$HOME/.cargo/bin:$PATH   # rustup stable; rust-toolchain.toml pins stable
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
npx -y lat.md check                  # knowledge base in lat.md/
```

Tests:

- unit tests in `src/` (slug, levenshtein, config, tokens and others)
- `tests/cli.rs`: e2e on temp roots built with `owo new` plus `tests/fixtures/aria/`
- `tests/examples.rs`: e2e on the committed `examples/thea` and `examples/reviewer`. Read-only commands run in place (`context --no-touch`); writing commands run on a temp copy.

After editing anything in `examples/`, run for each identity: `owo --root examples -i <slug> stamp && owo ... index && owo ... lint`. Run `owo ... review done` too if MEMORY.md changed. The examples test requires zero lint warnings.

## Conventions

- One module per job in `src/`; `main.rs` holds clap definitions and dispatch only. Module map: `lat.md/architecture.md`.
- Every command supports `--json`, which prints the envelope `{ok, data, warnings, errors}`. Exit codes: 0 ok, 1 problems, 2 usage, 3 config (`ConfigError`), 4 I/O.
- Path decides an entry's kind. Templates are embedded with `include_str!`; `<root>/.templates/` overrides them.
- Generated files carry no dates, so `--check` modes compare bytes.
- Rust only: no Python or other script runtimes in the repo. Use small commits and push to `main`; fmt, clippy and tests stay green on every commit.
- Docs are written positively and plainly: describe what the system is and does. Record decisions in `lat.md/decisions.md`, log progress in `NOTES.md` with a date, and update the status table below.
- `lat.md` rules: each section opens with a paragraph of at most 250 characters; link with `[[file#Section]]`; the index is `lat.md/lat.md`.
- Example identities hold invented people only. A user appears at most as a generic character without identifiers.

## Done

| Command | Notes |
| :- | :- |
| `new`, `list`, `add` (15 types), `slug`, `templates`, `config` | |
| `lint [--rule] [--changed]` | 16 rules; `--changed` = uncommitted in Git or drifting from stamps |
| `index [--check]`, `stamp [--check]` | |
| `resolve`, `show [--section]`, `links [--in|--out|--orphans]` | lint reuses `find.rs` |
| `context [--voice --look --project --with --notes --format --no-touch]` | tiers, Now block, KB pointer with warning, `work/state.json` touch |
| `state`, `recent [--days]` | |
| `search [--scope --limit]` | in-crate BM25, no cache |
| `review`, `review done [--through]` | packet as text or JSON |
| `soul note`, `soul history` | evidence must resolve; weak evidence warns |
| `changes [--since]` | Git log, else stamps |
| `export card [--spec v2|v3] [--out]`, `export soulspec --out` | |
| `skills/` | identity, identity-create, identity-ingest, identity-review, shared `identity/references/rules.md` |

## Left, in priority order

1. **`owo sync start` / `sync done`** (design 6.3, 7.3). Read a transcript (`claude-jsonl`, `opencode-json`, `jsonl`), keep a per-session bookmark in `<root>/.owo/sessions/<id>.json`, write the new slice to `<identity>/sources/<date>_<session>.md`, and print the ingest prompt path. `done --until <n>` moves the bookmark. This is the main integration point for hosts (T3 Code, Claude Code hooks).
2. **`owo migrate v1 <path> --out <root>/<slug>`** (design 11). Mechanical moves from the v1 layout per the table, then print the checklist. Test it on a synthetic v1 fixture created under `tests/fixtures/`. Never read real identity folders.
3. **`kb:` links** (design 10.5): `[[kb:<p>:<path>#<Section>]]`. Resolve `<p>` via `context::kb_path`; lint warns when the file or section is missing and the KB exists, and stays silent when the KB folder is absent.
4. **`owo export skill --out <dir>`**: a `SKILL.md` wrapper that runs `owo context -i <slug>` and lists the read and write commands (see `skills/identity/SKILL.md`).
5. **Card extras**: `--png <avatar>` embeds the JSON (`chara` tEXt chunk for V2, `ccv3` for V3, base64); `--scenario <event or location>` sets `scenario` from that entry's summary.
6. **Flag gaps against design 7.3**: `add project --kb <path>` (sets `kb:` in project.md); `search --type/--project/--since`; `state --days/--project`; `list` with display name, last activity and projects.
7. **`owo mcp`**: stdio MCP server exposing resolve, search, links, show, context, state, add, stamp, index, lint over the same functions. Pick a small MCP crate or hand-roll JSON-RPC.
8. **Search cache** in `<root>/.owo/cache/<slug>/` only if identities grow past a few thousand files; tantivy is the documented upgrade path.

## Where things are

- `src/`: see `lat.md/architecture.md`. Command reference: `lat.md/cli.md`. File formats: `lat.md/formats.md`. Lint rules: `lat.md/lint.md`.
- `docs/design.md`: full product design. Section numbers above refer to it.
- `templates/`: embedded templates. `skills/`: agent skills.
- `examples/`: committed example identities. `tests/`: e2e tests.
- `NOTES.md`: dated progress log.
