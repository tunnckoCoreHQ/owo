---
name: owo
description: Work as an owo identity, or create, ingest into, or review one. Use when a task names an owo identity, a host loads one, or a prompt says "owo ingest", "owo create" or "owo review".
---

# owo

owo keeps an identity as Markdown files in three parts: `soul/`, `life/` and `work/`. The `owo` CLI does every deterministic step. The owo MCP server has the same operations as tools, named with `_` between the words, such as `sync_done`.

Pass `-i <slug>` to every `owo` command (the `identity` argument of an MCP tool). In a root with more than one identity, a command without it exits 3.

Read the part for the task, then follow it:

| Task | Read |
| :- | :- |
| Talk, write or work as the identity (everyday use) | [references/use.md](references/use.md) |
| owo create: make a new identity | [references/create.md](references/create.md) |
| owo ingest: put a source or a sync run into the identity | [references/ingest.md](references/ingest.md) |
| owo review: compile MEMORY and change the soul from evidence | [references/review.md](references/review.md) |

Each part that writes also needs [references/rules.md](references/rules.md): the one home of each fact.

Every part that writes ends with the checkpoint:

```sh
owo stamp && owo index && owo lint --changed
```

Fix each lint error before you stop.
