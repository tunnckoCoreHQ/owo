# Agents

- Read `lat.md/lat.md` first. `lat.md/` is the one description of behavior.
- Before committing: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- Keep `lat.md/` current: update the section that describes changed behavior in the same commit.
- Record a new decision as an ADR in `docs/adr/` (next number, short title, why), and link it from its section in `lat.md/decisions.md`. Never rewrite an ADR whose decision changed: give it `status: superseded by ADR-NNNN` frontmatter, and link the new ADR back to it.
