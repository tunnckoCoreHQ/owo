---
name: owo
description: Load this when you embody an owo identity, or when a prompt asks for owo create, owo ingest or owo reflect.
---

# owo

An owo Identity is a folder of Markdown files in three Scopes: `persona/`, `life/` and `work/`. The `owo` CLI does every step that needs no judgment. The owo MCP server has the same Operations as tools, such as `sync_done` for `owo sync done`. When you have only that server and no shell, call the tool for each command in this skill, and read the files of this skill with the `guide` tool.

The words in capitals, such as Entry, Daily Log or Reflection, are the terms of [references/glossary.md](references/glossary.md). Name things with those terms when you write or report, and read the glossary when a term in this skill or in owo output is new to you.

In an Identities Dir with more than one Identity, name the Identity in each call. On the CLI that is `-i <slug>`, and on an MCP tool it is the `identity` argument. When the Host gave you an Identity, use that one. In such an Identities Dir, a CLI command that reads an Identity exits 3 when no Identity is selected.

## Routing

Take the path for the task:

| Task | Path |
| :- | :- |
| Embody the Identity, to use it or to assume it at the start of a thread | [references/embody.md](references/embody.md) |
| owo create, to make a new Identity | [references/create.md](references/create.md) |
| owo ingest, to put Raw Material or a Sync Run into the Identity | [references/ingest.md](references/ingest.md) |
| owo reflect, to compile MEMORY and change the Persona from Evidence | [references/reflect.md](references/reflect.md) |

Every path that writes follows [references/rules.md](references/rules.md), which gives each fact its one home. When you write, use the `unslop` skill if it is installed.

## Checkpoint

Every path that writes ends with the Checkpoint:

```sh
owo stamp && owo index && owo lint --changed
```

Fix each lint error before you stop.
