---
name: identity-review
description: Compile an owo identity's MEMORY and evolve its soul from evidence. Use on request, or when `owo state` shows review is due.
---

# Identity review

Rules for where facts live: [../identity/references/rules.md](../identity/references/rules.md).

1. `owo review` prints the packet: current MEMORY, files changed since the last review, unreviewed days, events, journal paths, open threads and conflicts, recurring phrases, the last soul change. `--json` gives the same data.
2. Read the files the packet lists that matter (`owo show <name>`).
3. Write or update lessons from repeated experience (`owo add lesson <title>`, with `evidence` links).
4. Recompile `soul/MEMORY.md`: patterns, defining moments, lesson links, hints. Links, not retellings. Stay near 1500 words.
5. Close threads that resolved: `state: closed`.
6. Change a soul file only when the evidence holds:
   - the same pattern in at least two events or three log days across at least two weeks, or
   - one event with `defining: true`.

   Keep each change small, and record it:

   ```sh
   owo soul note SOUL.md --change "<one line>" --evidence "[[Event]]" --evidence "[[Lesson]]"
   ```

7. Close the review and check:

```sh
owo review done
owo stamp && owo index && owo lint
```
