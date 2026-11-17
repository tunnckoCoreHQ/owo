# owo

owo means "one world, one identity". It is a CLI for living agent identities. Each Identity is a folder of Markdown and JSON with three Scopes: `persona/`, `life/` and `work/`. owo does the deterministic work (create, add, lint, index, stamp) so agents can focus on writing.

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

The `skills/owo/` folder holds the owo skill. Its entry file sends the agent to the path for the task: embody, owo create, owo ingest or owo reflect. Link the whole folder into the Host's skills folder, for example `ln -s "$PWD/skills/owo" ~/.claude/skills/owo` for Claude Code, or copy it with links dereferenced: `cp -rL skills/owo ~/.claude/skills/owo`. The file `references/glossary.md` in it is a relative symlink to the `GLOSSARY.md` of the repo, so a plain `cp -r` breaks it. The four older skills are in `_skills/` for reference.

## Sync hook

After each Turn, the Host calls owo with its session id, the Host's name for the Session. A Host with a Transcript on this machine passes its path:

```sh
owo -i thea sync start --session <session_id> --transcript <path> --format claude-jsonl --json
```

A Host with no Transcript on the machine, such as a hosted chat over MCP, pushes its recent Turns as a JSON array of `{id, role, text}` on stdin, where `id` is the turn id, the Host's own id for that message:

```sh
owo -i thea sync push --session <session_id> --json < turns.json
```

When the reply has `due: true`, start an Ingest Agent with the Ingest Prompt it names. The Ingest Prompt is the whole task of the Sync Run: it asks for owo ingest, and its last step is `owo sync done` for that run. With `owo acp`, set `auto_ingest = true` and the proxy starts the Ingest Agent itself. For Claude Code, put the `sync start` line in a `Stop` hook.

More: [lat.md/](lat.md/lat.md).
