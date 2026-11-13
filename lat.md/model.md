# Model

What an identity is, how it changes over time, and how hosts use it.

## Identity

An identity is a folder of Markdown and JSON that any agent host can load. It has three parts.

- `soul/` is who the identity is. It changes slowly, through review, with evidence.
- `life/` is what the identity has lived and knows, written from its own perspective in one world. Every person, including the person it spends most time with, is a character.
- `work/` is its professional side: a profile, and its own experience of each project. Facts about a project itself stay in that project's KB.

One identity works across many projects. The layout is in [[formats#Identity layout]].

## Principles

The rules that shape every format and command.

- Each fact has one home. Other files link to it. The routing table is in `skills/identity/references/rules.md`.
- People and agents write authored files. Only owo writes generated files: indexes, the timeline, state files, the Now block and exports.
- The CLI does the mechanics: create, validate, index, search, resolve, stamp, slice transcripts, export. Agents decide what is durable, write prose and compile MEMORY.
- Maintenance runs when a person, a host hook or an agent asks for it. Every maintenance command is safe to run at any time.
- Every command has text output for people and `--json` output for agents.

## Lifecycle

An identity is created once, then grows through sessions, ingest and review.

1. Create: `owo new`, then the `identity-create` skill fills the soul files, a first MEMORY and the first entries, and runs `owo review done` to record the first MEMORY hash.
2. Session: the host loads `owo context` (see [[cli#Context]]); the agent reads more with `resolve`, `show`, `search` and `links`, and writes facts to their homes. The `identity` skill holds the session procedure and the checkpoint.
3. Ingest: the `identity-ingest` skill turns a source file or a sync slice into entries that cite it. See [[cli#Sync]].
4. Review: when `owo state` shows review is due, the `identity-review` skill reads `owo review` (see [[cli#Review]]), writes lessons, recompiles MEMORY, closes threads, and changes soul files only past the evidence threshold, recording each change with `owo soul note`.

Conflicts between sources become `life/conflicts/` entries; the `identity` skill rules describe how they open and resolve.

## Integration

A host needs two hooks: one that loads `owo context` into the prompt, and one that calls `owo sync` after a turn.

- A host with a local transcript calls `owo sync start --session <session_id> --transcript <path> --format <fmt>` after each turn, runs a background agent with the ingest prompt and the `identity-ingest` skill when a run is due, and calls `owo sync done` on success. For Claude Code this is a `Stop` hook with `--format claude-jsonl`, and a `SessionStart` hook or the exported skill wraps `owo context`.
- A host with no transcript on this machine, such as a hosted chat over MCP, calls `owo sync push` with its recent turns.
- MCP hosts run `owo mcp`; see [[decisions#MCP calls the library]].
- Hosts that load skills use `skills/` or the wrapper from `owo export skill`.
- Character card and SoulSpec hosts take `owo export`; see [[cli#Export]].
