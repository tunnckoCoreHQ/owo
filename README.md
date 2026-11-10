# owo

owo means "one world, one identity". It is a CLI for living agent identities: folders of Markdown and JSON with `soul/`, `life/` and `work/`. owo does the deterministic work (create, add, lint, index, stamp) so agents can focus on writing.

## Install

```sh
cargo install --path .
```

## Use

```sh
owo --root ~/identities new "Thea"
owo -i thea add event "The Café Night" --date 2026-10-25
owo -i thea index && owo -i thea stamp && owo -i thea lint
```

## Skills

The `skills/` folder holds four skills: `identity`, `identity-create`, `identity-ingest` and `identity-review`. Copy or link each folder into the host's skills folder, for example `~/.claude/skills/` for Claude Code. Keep the four side by side, because the other three link to `identity/references/rules.md`.

## Sync hook

After each turn, the host calls owo with the session id. A host with a transcript file on this machine passes its path:

```sh
owo -i thea sync start --session <id> --transcript <path> --format claude-jsonl --json
```

A host with no local transcript, such as a hosted chat over MCP, sends its recent turns as a JSON array of `{id, role, text}` on stdin:

```sh
owo -i thea sync push --session <id> --json < turns.json
```

When the reply has `due: true`, run a background agent with the prompt it names and the `identity-ingest` skill, then call `owo -i thea sync done --session <id> --until <until>`. For Claude Code, put the `sync start` line in a `Stop` hook.

More: [lat.md/](lat.md/lat.md).
