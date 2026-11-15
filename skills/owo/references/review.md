# owo review

Use this part to compile `soul/MEMORY.md` and to change the soul from evidence: on request, or when `owo state` shows that review is due.

1. Run `owo review`. It prints the packet: the current MEMORY, the files changed since the last review, the days not reviewed, events, journal paths, open threads and conflicts, phrases that repeat, and the last soul change. `--json` gives the same data.
2. Read the files from the packet that matter, with `owo show <name>`.
3. Write or update a lesson for each experience that repeats: `owo add lesson <title>`, with `evidence` links.
4. Compile `soul/MEMORY.md` again: patterns, defining moments, links to lessons, hints. Write links, not retellings. Keep it near 1500 words.
5. Close each thread that is resolved: set `state: closed`.
6. Change a soul file only when the evidence holds:
   - the same pattern in two events or more, or on three log days or more, across two weeks or more, or
   - one event with `defining: true`.

   Keep each change small, and record it:

   ```sh
   owo soul note SOUL.md --change "<one line>" --evidence "[[Event]]" --evidence "[[Lesson]]"
   ```

7. Run the checkpoint from the [entry file](../SKILL.md). Then close the review with `owo review done`. The checkpoint stamps first, so the edits of this review do not show as changed in the next one.

Follow [rules.md](rules.md) for where each fact goes.
