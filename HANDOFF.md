# Handoff

## What owo is

owo is a Rust CLI for living agent identities. An identity is `<root>/<slug>/` with `soul/` (who it is), `life/` (what it lived and knows) and `work/` (professional side, per project). owo does every deterministic job; agent skills do the judgment work. Full design: `docs/design.md`.

## Build and test

```sh
cargo build
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`rust-toolchain.toml` pins stable with rustfmt and clippy.

## Status per command

| Command | Status |
| :- | :- |
| `owo new <name> [--slug]` | done |
| `owo add <type> <title> [--date --parent --alias --project]` | done, all 15 entry types |
| `owo lint [--rule]` | done: tree, schema, soul-plain, links, unique, identifiers, journal, supersede, generated, memory-review, state, budget, dupes, orphans, soul-evidence, review-due |
| `owo index [--check]` | done: category indexes, life index, timeline by era, work index |
| `owo stamp [--check]` | done: hashes, created/updated, frontmatter `updated`, soul.json minor bump |
| `owo list`, `owo slug`, `owo templates`, `owo config` | done |
| `--json`, exit codes 0/1/2/3/4 | done |
| `owo lint --changed` | todo |
| `kb:` link resolution | todo (links are skipped) |
| `owo resolve`, `owo show`, `owo links [--orphans]` | done |
| `owo search [--scope --limit]` | done, in-crate BM25 |
| `owo context [--voice --look --with --notes --format --no-touch]` | done: tiers, Now block, project files, KB pointer, work/state.json touch |
| `owo state`, `owo recent` | done |
| `owo changes` | todo |
| `owo review done` | done |
| `owo review` packet, `owo soul note`, `owo soul history` | todo |
| `owo sync start/done` | todo |
| `owo export card/soulspec/skill` | todo |
| `owo migrate v1` | todo |
| `owo mcp` | later |
| Skills rewrite (identity, identity-create, identity-ingest, identity-review) | todo |

## Suggested next phase

1. `owo review` packet (unreviewed logs, journal, notes since `reviewed_through`) and `soul note/history`.
2. `owo changes`, `owo lint --changed`, `kb:` link resolution.
3. Lint reuses `find::orphans` and `find::file_links` (lint still has its own copies).
4. `sync start/done`, `export`, `migrate v1`, then the skills rewrite.

## Where things are

- `src/`: one module per job, see `lat.md/architecture.md`.
- `templates/`: embedded templates.
- `tests/cli.rs`, `tests/fixtures/aria/`: end-to-end tests.
- `examples/thea`, `examples/reviewer`: committed identities; `tests/examples.rs` runs e2e tests on them.
- `lat.md/`: knowledge base. `NOTES.md`: dated progress log.
