# owo

owo means "one world, one identity". It is a CLI for living agent identities: folders of Markdown and JSON with `soul/`, `life/` and `work/`. owo does the deterministic work (create, add, lint, index, stamp) so agents can focus on writing.

## Install

```sh
cargo install --path crates/cli
```

## Use

```sh
owo --root ~/identities new "Thea"
owo -i thea add event "The Café Night" --date 2026-10-25
owo -i thea index && owo -i thea stamp && owo -i thea lint
```

## Skills

The `skills/owo/` folder holds the owo skill. Copy or link it into the host's skills folder, for example `~/.claude/skills/owo` for Claude Code. Its entry file sends the agent to the path for the task: embody, owo create, owo ingest or owo review. The four older skills are in `_skills/` for reference.

## Sync hook

After each turn, the host calls owo with its session id, the host's name for the chat. A host with a transcript file on this machine passes its path:

```sh
owo -i thea sync start --session <session_id> --transcript <path> --format claude-jsonl --json
```

A host with no local transcript, such as a hosted chat over MCP, sends its recent turns as a JSON array of `{id, role, text}` on stdin, where `id` is the turn id, the host's own id for that message:

```sh
owo -i thea sync push --session <session_id> --json < turns.json
```

When the reply has `due: true`, run a background agent with the prompt it names. The prompt is the whole task: it asks for owo ingest, and its last step is `owo sync done` for that run. With `owo acp`, set `auto_ingest = true` and the proxy runs that agent itself. For Claude Code, put the `sync start` line in a `Stop` hook.

More: [lat.md/](lat.md/lat.md).
