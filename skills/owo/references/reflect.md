# owo reflect

Run a Reflection: compile `persona/MEMORY.md` again, and change the Persona from Evidence. Do it when the user asks, or when `owo state` shows that a Reflection is due.

## Read

Run `owo reflect`. It prints the Reflection Digest: the current MEMORY, the Entries changed since the last Reflection, the Daily Logs since then, Events, Journal paths, open Threads and Conflicts, phrases that repeat, and the last Persona Change. `--json` gives the same data. Read the Entries in the digest that matter, with `owo show <name>`.

## Compile

- Write or update an Insight for each experience that repeats, with `owo add insight <title>` and `evidence` Wikilinks.
- Compile `persona/MEMORY.md` again, with patterns, defining moments, Wikilinks to Insights and hints. Write Wikilinks, not retellings. Keep it near 1500 words.
- Close each Thread that is resolved. Set `state: closed` on it.

## Change the Persona

Change a Persona File only when one of these is true:

- The pattern shows up on three different days or more, in Daily Logs or in Events, and the first and the last are two weeks apart or more.
- One Event has `defining: true`.

Keep each change small, and record it as a Persona Change:

```sh
owo persona change SOUL.md --change "<one line>" --evidence "[[Event]]" --evidence "[[Insight]]"
```

## Finish

Run the skill's **Checkpoint**, which stamps, indexes and lints the changes. Then close the Reflection with `owo reflect done`. The Checkpoint stamps first, so the edits of this Reflection do not show as changed in the next one.

Follow [rules.md](rules.md) for where each fact goes.
