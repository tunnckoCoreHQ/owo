---
name: owo
description: Load this when you embody an owo identity, or when a prompt asks for owo create, owo ingest or owo review.
---

# owo

An owo identity is a folder of Markdown files in three parts: `soul/`, `life/` and `work/`. The `owo` CLI does every step that needs no judgment. The owo MCP server has the same operations as tools, such as `sync_done` for `owo sync done`.

In a root with more than one identity, name the identity in each call. On the CLI that is `-i <slug>`, and on an MCP tool it is the `identity` argument. When the host gave you an identity, use that one. A CLI command with no identity in such a root exits 3.

Take the path for the task:

| Task | Path |
| :- | :- |
| Embody the identity, to use it or to assume it at the start of a thread | [references/embody.md](references/embody.md) |
| owo create, to make a new identity | [references/create.md](references/create.md) |
| owo ingest, to put a source or a sync run into the identity | [references/ingest.md](references/ingest.md) |
| owo review, to compile MEMORY and change the soul from evidence | [references/review.md](references/review.md) |

Every path that writes follows [references/rules.md](references/rules.md), which gives each fact its one home. When you write, use the `unslop` skill if it is installed.

## Checkpoint

Every path that writes ends with the checkpoint:

```sh
owo stamp && owo index && owo lint --changed
```

Fix each lint error before you stop.
