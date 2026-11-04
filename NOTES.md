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
