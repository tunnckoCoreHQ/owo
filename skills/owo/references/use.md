# Everyday use

Use this part when you talk, write or work as the identity.

## Load

Load the identity before you answer as it:

```sh
owo context [--voice] [--look] [--project <p>]
```

- Add `--voice` when you talk or write as the identity.
- Add `--look` when appearance matters: images, scenes, video.
- Add `--project <p>` for work on a project. The output then has `work/README.md`, the `project.md`, recent notes and a pointer to the project's knowledge base. Get the facts of the project from that knowledge base.

Keep to the identity for the whole session: its voice, values and boundaries.

## Find more

Read only what the task needs:

- `owo resolve <name>`: the path of an entry.
- `owo show <name> [--section <s>]`: an entry, or one section of it.
- `owo search <words> [--scope life|work|soul]`: ranked matches, each with its best line.
- `owo links <name>`: what links to an entry, and what it links to.
- `owo state`: the Now block. `owo changes --days <n>`: the files that changed recently.

## Write

Write a fact only in its home. [rules.md](rules.md) has the table of homes.

1. Find the home first with `owo resolve` or `owo search`.
2. When the home exists, edit it in place, with your file tools or `owo write <path>` (the full new text).
3. When it does not exist, make it with `owo add <type> <title>`. owo picks the path and the template. Fill the sections of the template.
4. Put a small thing in today's log: `owo add log`. When today's log exists, it prints `exists <path>`; then edit that file.

Do not change `soul/MEMORY.md` or the soul files here. That is [review](review.md).

At a checkpoint, or at the end of the session, run the checkpoint from the [entry file](../SKILL.md).
