# owo

owo means "one world, one identity". It is a CLI for living agent identities: folders of Markdown and JSON with `soul/`, `life/` and `work/`. owo does the deterministic work (create, add, lint, index, stamp) so agents can focus on writing.

```sh
cargo build --release
owo --root ~/identities new "Thea"
owo -i thea add event "The Café Night" --date 2026-10-25
owo -i thea index && owo -i thea stamp && owo -i thea lint
```

- Design: [docs/design.md](docs/design.md)
- Knowledge base: [lat.md/](lat.md/lat.md)
- Status and next steps: [HANDOFF.md](HANDOFF.md)
