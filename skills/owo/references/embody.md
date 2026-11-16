# Embody

Embody the Identity when the user asks you to use it or to assume it, or when the Host loads it. That usually happens once, at the start of a thread.

## Load

Load the Context once, before you answer as the Identity:

```sh
owo context [--voice] [--look] [--project <p>]
```

The core Tier always loads. Each flag adds one more Tier:

- `--voice` adds the voice Tier, how the Identity talks and writes. Add it when you talk or write as the Identity.
- `--look` adds the look Tier, how the Identity looks. Add it when the thread is about images, scenes or video.
- `--project <p>` adds the work Tier: the Work Profile, the Project Readme, recent Project Notes and the Project Wiki Path. Add it for work on a Project, and take the facts of the Project from its Project Wiki, with `owo show wiki:<p>:<path>`.

When the Host already put the Identity in the prompt, or told you how to load it, do that instead.

Stay the Identity for the whole thread: its voice, its values and its boundaries.

Later in the thread, run `owo context` again only when you need the whole Context again. To read one Persona File, run `owo persona` with the flag of that file:

- `--id`: IDENTITY, who the Identity is.
- `--soul`: SOUL, its beliefs, wants and boundaries.
- `--voice`: VOICE, how it talks and writes.
- `--look`: LOOK, how it looks.
- `--memory`: MEMORY, its compiled long-term memory.

## Find more

Read only what the task needs:

- `owo resolve <name>` prints the path of an Entry.
- `owo show <name> [--section <s>]` prints an Entry, or one section of it.
- `owo search <words> [--scope life|work|persona]` prints ranked matches, each with its best line.
- `owo links <name>` prints the Wikilinks to an Entry and the Wikilinks in it.
- `owo state` prints the Current State, and `owo changes --days <n>` prints the files that changed recently.

## Write

Write only when the user asks you to remember or record something. A Host that syncs the thread, such as `owo acp` or a sync hook, keeps the Turns, and owo ingest records them later.

- Find the home of the fact first, with `owo resolve` or `owo search`. [rules.md](rules.md) has the table of homes.
- When the home exists, edit it in place with your file tools, or give `owo write <path>` the full new text of the file.
- When it does not exist, make it with `owo add <kind> <title>`, and fill the sections of the template.
- Put a small thing as one bullet in today's Daily Log. Run `owo add daily-log`, which makes the Daily Log or prints `exists <path>` when it is there already. Then add the bullet to that file.
- The Persona Files, MEMORY included, change only on the [reflect path](reflect.md).

After you write, run the skill's **Checkpoint**, which stamps, indexes and lints the changes.
