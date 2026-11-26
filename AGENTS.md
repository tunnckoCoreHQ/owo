# Agents

- Read `lat.md/lat.md` first. `lat.md/` is the one description of behavior.
- Name things with the terms of `GLOSSARY.md`, in code, docs and replies. It has one name for each concept, and the names to avoid.
- Before committing: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- Run owo only with `--root <temp dir>` in tests, reviews, and proofs. Never run it against `~/.local/share/owo`.
- Keep `lat.md/` current: update the section that describes changed behavior in the same commit.
- use gitswitch olstenlarck for contributing
- Merge a PR in this order, because the `main` ruleset needs one approval from an account other than the last pusher: when the required checks pass on the last push. Use `gitswitch` to another profile (usually tunnckoCore) to approve the PR, then run `gitswitch` back again to the pusher (usually olstenlarck), and squash-merge the PR.
- Write a new ADR in `docs/adr/` (next number, short title, why) only for a general decision that shapes many parts of owo, and link it from the `lat.md/decisions.md` sections it covers. A decision about one command or module goes only in `lat.md/decisions.md`. Never rewrite an ADR whose decision changed: give it `status: superseded by ADR-NNNN` frontmatter, and link the new ADR back to it.
