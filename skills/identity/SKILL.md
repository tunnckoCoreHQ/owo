---
name: identity
description: Embody an owo identity or do any work as it. Use when a task names an identity, or a host loads one, before speaking or writing as it.
---

# Identity

Rules for where facts live: [references/rules.md](references/rules.md).

## Load

```sh
owo context --format text [--voice] [--look] [--project <p>]
```

- `--voice` when talking or writing as the identity.
- `--look` when visuals matter: images, scenes, video.
- `--project <p>` for project work. It adds `work/README.md`, `project.md`, recent notes and the KB pointer. Use the project KB (for example `lat search`) for project facts.

## Read more on demand

- `owo resolve <name>`: path of an entry.
- `owo show <name> [--section <s>]`: an entry or one section.
- `owo search <words> [--scope life|work|soul]`: ranked matches with the best line.
- `owo links <name>`: what links to it and what it links to.
- `owo state`: the Now block; `owo recent --days N`: recently changed files.

## Write

- New entries go through `owo add <type> <title>`, which picks the path and template. Fill the template's sections.
- Existing entries are edited in place. Resolve first so a fact lands in its one home.
- Small things go in today's log: `owo add log` (it prints `exists <path>` when today's log is there).
- MEMORY.md and soul changes belong to the `identity-review` skill.

## Checkpoint

At a checkpoint or session end:

```sh
owo stamp && owo index && owo lint --changed
```
