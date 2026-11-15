# owo ingest

Use this part to put a source into the identity: a transcript, notes, a document, or the slice of a sync run.

## The source

- A sync run gives you its source: a slice in `<root>/sources/<slug>/`, and a prompt that names it. The prompt is the whole task. Do each of its steps, the last one too: `owo sync done` for that run.
- A `sync_push` result that is due has the slice as `source_text` and the prompt as `prompt_text`, for an agent that cannot read the files. Cite the `source` path of the result.
- Any other file: copy it into `<root>/sources/<slug>/` first, and cite it by its path relative to `sources/`.

## Steps

1. Run `owo context` and `owo state`, to know the identity and what is open now.
2. Read the source. List its durable facts: happenings, people, places, decisions, quotes, work done, promises still open. Leave out small talk and facts that are already in the identity.
3. For each fact, find its home with `owo resolve <name>` or `owo search <words>`. [rules.md](rules.md) has the table of homes.
   - When the home exists, read it with `owo show`, and merge the fact in place.
   - When it does not exist, make it with `owo add <type> <title> [--date <day>] [--project <p>]`, and fill the template.
   - Add the source to the frontmatter `sources` list of the entry, with the lines it came from: `<slug>/<file>.md#L<a>-L<b>`.
4. Put a small thing as one bullet in the log of the day it happened (`owo add log --date <day>`). An event gets one bullet in that log, with a link to it.
5. When the source disagrees with an entry, record a conflict (`owo add conflict`). Do not overwrite the entry.
6. Do not change `soul/MEMORY.md` or the soul files. That is [review](review.md).
7. Run the checkpoint from the [entry file](../SKILL.md). Report the paths that `owo stamp` lists, or "no changes".
8. For a sync run, run the `owo sync done` command of the prompt, only after the checkpoint passes.
