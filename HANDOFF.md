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
| `owo resolve`, `owo show`, `owo links` | todo |
| `owo search` (BM25, tantivy) | todo |
| `owo context` (tiers, generated context block, work/state.json touch) | todo |
| `owo state`, `owo recent`, `owo changes` | todo |
| `owo review`, `owo review done`, `owo soul note`, `owo soul history` | todo |
| `owo sync start/done` | todo |
| `owo export card/soulspec/skill` | todo |
| `owo migrate v1` | todo |
| `owo mcp` | later |
| Skills rewrite (identity, identity-create, identity-ingest, identity-review) | todo |

## Suggested next phase

1. `resolve`, `show`, `links` (reuse `Identity::link_keys`).
2. `context` with tiers and the generated "Now" block, and `state`.
3. `review` packet, `review done`, `soul note/history`.
4. `search` with tantivy, cache in `<root>/.owo/cache/<slug>/`.

## Where things are

- `src/`: one module per job, see `lat.md/architecture.md`.
- `templates/`: embedded templates.
- `tests/cli.rs`, `tests/fixtures/aria/`: end-to-end tests.
- `lat.md/`: knowledge base. `NOTES.md`: dated progress log.
