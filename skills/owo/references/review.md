# owo review

Compile `soul/MEMORY.md`, and change the soul from evidence. Do it when the user asks, or when `owo state` shows that a review is due.

## Read

Run `owo review`. It prints the packet: the current MEMORY, the files changed since the last review, the days not reviewed, events, journal paths, open threads and conflicts, phrases that repeat, and the last soul change. `--json` gives the same data. Read the files in the packet that matter, with `owo show <name>`.

## Compile

- Write or update a lesson for each experience that repeats, with `owo add lesson <title>` and `evidence` links.
- Compile `soul/MEMORY.md` again, with patterns, defining moments, links to lessons and hints. Write links, not retellings. Keep it near 1500 words.
- Close each thread that is resolved. Set `state: closed` on it.

## Change the soul

Change a soul file only when one of these is true:

- The pattern shows up on three different days or more, in logs or in events, and the first and the last are two weeks apart or more.
- One event has `defining: true`.

Keep each change small, and record it:

```sh
owo soul note SOUL.md --change "<one line>" --evidence "[[Event]]" --evidence "[[Lesson]]"
```

## Finish

Run the [checkpoint](../SKILL.md#checkpoint), which stamps, indexes and lints the changes. Then close the review with `owo review done`. The checkpoint stamps first, so the edits of this review do not show as changed in the next one.

Follow [rules.md](rules.md) for where each fact goes.
