---
name: identity-ingest
description: Import a transcript, notes or documents into an owo identity, or run a sync after a session.
---

# Identity ingest

Rules for where facts live: [../identity/references/rules.md](../identity/references/rules.md).

Input: a source file path. Copy a file from elsewhere into `<root>/sources/<slug>/` first and cite it relative to `sources/`.

A `sync_push` reply that is due carries the slice as `source_text` and the prompt as `prompt_text`; cite its `source` path and finish with `sync_done`.

1. Read the core with `owo context --format text --no-touch` and the current state with `owo state`.
2. Read the source. List the durable facts: happenings, people, places, decisions, quotes, work done, open promises.
3. For each fact:
   - `owo resolve <name>` or `owo search <words>` to find its home.
   - If the home exists, `owo show` it and merge the fact in place.
   - Otherwise `owo add <type> <title> [--date] [--project]` and fill the template.
   - Add the source path, with its line range, to the entry's frontmatter `sources` list.
4. Small things become one bullet in that day's log (`owo add log --date <day>`); events get one bullet linking them.
5. Record disagreements with existing entries as conflicts (`owo add conflict`).
6. Leave MEMORY.md and soul files to review.
7. Finish and report the changed paths, or "no changes":

```sh
owo stamp && owo index && owo lint --changed
owo changes
```
