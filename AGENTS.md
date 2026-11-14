# Agents

- Read `lat.md/lat.md` first. `lat.md/` is the one description of behavior.
- Before committing: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- Keep `lat.md/` current: update the section that describes changed behavior in the same commit.
- use gitswitch olstenlarck for contributing
- Write a new ADR in `docs/adr/` (next number, short title, why) only for a general decision that shapes many parts of owo, and link it from the `lat.md/decisions.md` sections it covers. A decision about one command or module goes only in `lat.md/decisions.md`. Never rewrite an ADR whose decision changed: give it `status: superseded by ADR-NNNN` frontmatter, and link the new ADR back to it.
