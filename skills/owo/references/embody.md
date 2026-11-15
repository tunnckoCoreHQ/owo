# Embody

Embody the identity when the user asks you to use it or to assume it, or when the host loads it. That usually happens once, at the start of a thread.

## Load

Load the identity once, before you answer as it:

```sh
owo context [--voice] [--look] [--project <p>]
```

- `--voice` adds how the identity talks and writes. Add it when you talk or write as the identity.
- `--look` adds how the identity looks. Add it when the thread is about images, scenes or video.
- `--project <p>` adds `work/README.md`, the `project.md`, recent notes and a pointer to the knowledge base of the project. Add it for work on a project, and take the facts of the project from that knowledge base.

When the host already put the identity in the prompt, or told you how to load it, do that instead.

Stay the identity for the whole thread: its voice, its values and its boundaries.

## Find more

Read only what the task needs:

- `owo resolve <name>` prints the path of an entry.
- `owo show <name> [--section <s>]` prints an entry, or one section of it.
- `owo search <words> [--scope life|work|soul]` prints ranked matches, each with its best line.
- `owo links <name>` prints what links to an entry, and what it links to.
- `owo state` prints the Now block, and `owo changes --days <n>` prints the files that changed recently.

## Write

Write only when the user asks you to remember or record something. A host that syncs the thread, such as `owo acp` or a sync hook, stores the turns, and owo ingest records them later.

- Find the home of the fact first, with `owo resolve` or `owo search`. [rules.md](rules.md) has the table of homes.
- When the home exists, edit it in place with your file tools, or give `owo write <path>` the full new text of the file.
- When it does not exist, make it with `owo add <type> <title>`, and fill the sections of the template.
- Put a small thing as one bullet in today's log. Run `owo add log`, which makes the log or prints `exists <path>` when it is there already. Then add the bullet to that file.
- Do not change `soul/MEMORY.md` or the soul files. That is the [review path](review.md).

End with the checkpoint.
