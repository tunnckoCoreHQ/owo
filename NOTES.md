# Notes

## 2026-11-04 (phase 1)

- Created the crate: clap CLI, config resolution, embedded templates, `new`, `add`, `lint` (16 rules), `index`, `stamp`, `list`, `slug`, `templates`, `config`.
- Integration tests in `tests/cli.rs` build identities in temp roots; `tests/fixtures/aria/` is a small neutral identity copied over a fresh `owo new`.
- `lat.md/` documents architecture, formats, CLI, lint rules and decisions. `lat check` has not run yet (lat is an npm tool; run it where lat is installed).
- `docs/design.md` copied from the design doc (gist d15503942e4ece3c23d71779547d2390).
