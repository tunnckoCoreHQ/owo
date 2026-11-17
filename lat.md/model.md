# Model

What an identity is, how it changes over time, and how hosts use it.

## Identity

An identity is a folder of Markdown and JSON that any agent host can load. It has three scopes.

- `persona/` is who the identity is. It changes slowly, only through a reflection, with evidence.
- `life/` is what the identity has lived and knows, written from its own perspective in one world. Every person, including the person it spends most time with, is a character.
- `work/` is its professional side: a profile, and its own experience of each project. Facts about a project itself stay in its project wiki.

One identity works across many projects. The layout is in [[formats#Identity layout]].

## Principles

The rules that shape every format and command.

- Each fact has one home. Other files link to it. The table of homes is in `skills/owo/references/rules.md`.
- People and agents write authored files. Only owo writes the rest: the generated files, the state files, the current state and the exports.
- The CLI does the mechanics: create, validate, index, search, resolve, stamp, slice transcripts, export. Agents decide what is durable, write prose and compile MEMORY.
- Maintenance runs when a person, a host hook or an agent asks for it. Every maintenance command is safe to run at any time.
- Every command has text output for people and `--json` output for agents.

## Lifecycle

An identity is created once, then grows through sessions, ingest and reflection.

1. Create: `owo new`, then owo create (the create path of the `owo` skill) fills the persona files, the work profile and characters when they apply, and a first MEMORY, and runs `owo reflect done` to record the first MEMORY hash.
2. Session: the host loads `owo context` (see [[cli#Context]]); the agent embodies the identity and reads more with `resolve`, `show`, `search` and `links`; it writes only when the user asks, because sync and ingest record the session. The embody path of the `owo` skill holds this, and its entry file the checkpoint.
3. Ingest: owo ingest (the ingest path of the `owo` skill) turns raw material, such as a slice, into entries that cite it. See [[cli#Sync]].
4. Reflection: when `owo state` shows a reflection is due, owo reflect (the reflect path of the `owo` skill) reads the digest of `owo reflect` (see [[cli#Reflect]]), writes insights, recompiles MEMORY, closes threads, and changes persona files only past the evidence threshold, recording each change with `owo persona change`.

Conflicts between pieces of raw material become `life/conflicts/` entries; the rules of the `owo` skill describe how they open and resolve.

## Integration

A host needs two hooks: one that loads `owo context` into the prompt, and one that calls `owo sync` after a turn. A local host has owo and the identities dir on its machine, and its agent runs owo commands. A remote host reaches owo only through an MCP server, and its agent calls tools.

- A local host with a transcript calls `owo sync start --session <session_id> --transcript <path> --format <fmt>` after each turn, and starts an ingest agent in the background with the ingest prompt when a run is due; the prompt asks for owo ingest and ends with `owo sync done`. For Claude Code this is a `Stop` hook with `--format claude-jsonl`, and a `SessionStart` hook or the exported skill wraps `owo context`.
- A remote host, such as a hosted chat, calls the `sync_push` tool with its recent turns. When the result has `prompt_text`, its agent does that ingest with the tools of the same server, and reads the skill with the `guide` tool; see [[decisions#The ingest prompt names its interface]]. A local host with no transcript file calls `owo sync push` the same way.
- MCP hosts run `owo mcp`; see [[decisions#MCP calls the library]].
- ACP clients run the agent through `owo acp`, which stores the turns with `owo sync push` and gives the agent the identity, and with `auto_ingest` runs the ingest of each due or pending run in the background; see [[decisions#ACP proxy passes lines through]] and [[decisions#ACP proxy runs the ingest in the background]].
- Hosts that load skills use `skills/owo/` or the wrapper from `owo export skill`.
- Hosts that read Character Card V2 or V3, or SoulSpec, take `owo export`; see [[cli#Export]].
