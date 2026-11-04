# Handoff

A standalone brief for the next coding agent. Read this, then `lat.md/lat.md`, then the sections of `docs/design.md` named per task.

## What owo is

owo ("one world, one identity") is a Rust CLI for living agent identities. An identity is a folder `<root>/<slug>/`:

- `identity.json`: name, optional `nickname`, accounts, budgets, `review_every_days`, `sync_every`, `projects.<p>.kb`.
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
- `tests/phase4.rs`: e2e for sync, migrate, kb links, export skill and PNG cards, and the newer flags.
- `tests/mcp.rs`: e2e for `owo mcp` over stdin and stdout.
- `tests/examples.rs`: e2e on the committed `examples/nicole` and `examples/reviewer`. Read-only commands run in place (`context --no-touch`); writing commands run on a temp copy.

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
| `new`, `add` (15 types, `--kb` for projects), `slug`, `templates [list\|show]`, `config` | a log without a title is titled by its date; unknown types exit 2 |
| `list` | slug, name, last activity, projects |
| `lint [--rule] [--changed]` | 17 rules incl. `kb`; `links` also checks `#Section` at any heading level; `--changed` = uncommitted in Git or drifting from stamps |
| `index [--check]`, `stamp [--check]` | timeline resolves `era` like a link; stamp keeps CRLF |
| `resolve`, `show [--section]`, `links [--in\|--out\|--orphans]` | `show kb:<p>:<path>#<S>` prints from the project KB |
| `context [--voice --look --project --with --notes --format --no-touch]` | tiers, Now block, KB pointer with warning (`~/` expands), `work/state.json` touch |
| `state [--days] [--project]`, `recent [--days]` | |
| `search [--scope/--in --type --since --limit]`, `--project` flag as a filter | in-crate BM25, no cache; snippets from the body |
| `review`, `review done [--through]` | `reviewed_through` defaults to the newest log, event or journal date |
| `soul note`, `soul history` | evidence must resolve; distinct entries count; weak evidence warns |
| `changes [--since]` | Git log plus uncommitted files, else stamps |
| `sync start`, `sync done` | jsonl, claude-jsonl, opencode-json; slices in `<root>/sources/<slug>/`; session lock; a pending run is not restarted each turn; `done --until` bounded by the messages read |
| `migrate v1` | locations, cultures, factions, eras, threads included; tested on `tests/fixtures/v1-lumen` (invented) |
| `export card [--spec --scenario --png --out]`, `export soulspec`, `export skill` | `nickname` from `identity.json` |
| `mcp [--remote [ADDR]] [--port]` | every command as a tool; stdio, or HTTP on 4224 with the URL printed; each tool runs the same binary; no auth |
| `skills/` | identity, identity-create, identity-ingest, identity-review, shared `identity/references/rules.md` |

## Left, in priority order

1. **Search cache** in `<root>/.owo/cache/<slug>/` only if identities grow past a few thousand files; tantivy is the documented upgrade path.

## Where things are

- `src/`: see `lat.md/architecture.md`. Command reference: `lat.md/cli.md`. File formats: `lat.md/formats.md`. Lint rules: `lat.md/lint.md`.
- `docs/design.md`: full product design. Section numbers above refer to it.
- `templates/`: embedded templates. `skills/`: agent skills.
- `examples/`: committed example identities. `tests/`: e2e tests.
- `NOTES.md`: dated progress log.
