# Notes

## 2026-11-04 (phase 1)

- Created the crate: clap CLI, config resolution, embedded templates, `new`, `add`, `lint` (16 rules), `index`, `stamp`, `list`, `slug`, `templates`, `config`.
- Integration tests in `tests/cli.rs` build identities in temp roots; `tests/fixtures/aria/` is a small neutral identity copied over a fresh `owo new`.
- `lat.md/` documents architecture, formats, CLI, lint rules and decisions. `lat check` has not run yet (lat is an npm tool; run it where lat is installed).
- `docs/design.md` copied from the design doc (gist d15503942e4ece3c23d71779547d2390).

## 2026-11-04 (phase 2)

- Added `resolve`, `show`, `links` (`--in`, `--out`, `--orphans`), `context`, `state`, `recent`, `search`, `review done`.
- `context` prints core files, the Now block, `--voice`/`--look`, and with `--project` the work profile, project.md, recent notes and the KB pointer. KB path order: project.md `kb`, identity.json, `.owo.toml`.
- Search is a small BM25 in `search.rs` (see `lat.md/decisions.md`).
- `examples/thea` (neutral creative companion) and `examples/reviewer` (made-up senior code reviewer, 3 projects with missing KB paths) built with `owo new` and `owo add`, then written by hand. Both lint clean.
- `index -q` and `stamp -q` are now quiet.
- `npx -y lat.md check` passes.

## 2026-11-04 (phase 3)

- Lint's link and orphan checks now call `find.rs` (`raw_links`, `orphans`).
- Added the `owo review` packet, `review done --through`, `soul note`, `soul history`, `changes [--since]` (Git or stamps), `lint --changed`.
- Added `export card` (V3 with V2 fields, or V2; `--project` scopes the lorebook and scenario) and `export soulspec`.
- Examples gained greetings, good and bad examples, filled `soul.json`, and one soul history record each.
- `tests/examples.rs` covers card shape, the SoulSpec package, review packet then done, soul notes, changes and `lint --changed`.
- Wrote the four skills under `skills/` with one shared `rules.md`.
- HANDOFF.md rewritten as a standalone brief with the remaining work in priority order.

## 2026-11-04 (phase 4)

- `sync start/done`: host-agnostic transcript slicing into `<root>/sources/<slug>/`, bookmark and prompt in `.owo/sessions/`.
- `migrate v1`, tested on the invented `tests/fixtures/v1-lumen`.
- `kb:` links: resolution in `find::kb_link`, lint rule `kb`, `owo show kb:...`. Links now keep their `#section`; `links` warns on a missing section.
- `export skill`, `export card --png/--scenario`.
- Flags: `add --kb`, `search --type/--since/--in` plus `--project`, `state --days/--project`, `list` with name, last activity and projects.
- Self-review: removed the `all_links` and `links_in_value` wrappers; the findings left open are listed in HANDOFF.md.

## 2026-11-04 (phase 5: review pass and handoff items)

- Review fixes: a log without a title is titled by its date; `project.md` resolves by its folder slug; sections match any heading level and skip fenced code (also for `show kb:` and the `kb` rule); the timeline resolves `era` like a link; identifier values may not leave their own `## Identifiers` section; stamp keeps CRLF line endings; soul evidence counts distinct entries; alias and `--kb` values escape backslashes; search snippets come from the body; `~/` expands in KB paths; `export card --json` prints the envelope.
- Exit codes: a missing identity folder exits 3, an unknown `add` type exits 2, and a closed stdout ends quietly.
- `migrate v1` maps location folders and loose location files (rooms become location details) and the cultures, factions, eras and threads folders.
- `review done` defaults `reviewed_through` to the newest log, event or journal date; the design now says so.
- `owo templates list|show`, with overrides by identity-relative path or `<type>.md`.
- One rule for the project: the working project resolves from flag, env, `.owo.toml`; `search` and `export card` filter only on the flag.
- Card `nickname` comes from `identity.json`; recurring phrases must start and end with a content word.
- `sync`: a session lock file, and a pending run is not started again on every turn.
- `owo mcp`: stdio JSON-RPC server; each tool runs the same binary.
