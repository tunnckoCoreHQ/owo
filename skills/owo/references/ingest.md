# owo ingest

Put a source into the identity. A source is a transcript, notes, a document, or the slice of a sync run.

## The source

- A sync run gives you a prompt and a slice in `<root>/sources/<slug>/`. The prompt is the whole task. Do every step of it, the last one too, which is `owo sync done` for that run.
- A `sync_push` result that is due has the slice in `source_text` and the prompt in `prompt_text`, for an agent that cannot read the files. Cite the `source` path of the result.
- Copy any other file into `<root>/sources/<slug>/` first, and cite it by its path relative to `sources/`.

## Read

Run `owo context` and `owo state` first, so you know the identity and what is open now. Then read the source, and list its durable facts. These are happenings, people, places, decisions, quotes, work done and promises still open. Leave out small talk and facts the identity has already.

## Record each fact

- Find the home of the fact with `owo resolve <name>` or `owo search <words>`. [rules.md](rules.md) has the table of homes.
- When the home exists, read it with `owo show`, and merge the fact in place.
- When it does not exist, make it with `owo add <type> <title> [--date <day>] [--project <p>]`, and fill the template.
- Add the source to the `sources` list in the frontmatter of the entry, with the lines it came from: `<slug>/<file>.md#L<a>-L<b>`.
- Put a small thing as one bullet in the log of the day it happened, with `owo add log --date <day>`. An event gets one bullet in that log, with a link to it.
- When the source disagrees with an entry, record a conflict with `owo add conflict`. Do not overwrite the entry.
- Do not change `soul/MEMORY.md` or the soul files. That is the [review path](review.md).

## Finish

Run the checkpoint. Report the paths that `owo stamp` lists, or "no changes". For a sync run, run its `owo sync done` command after the checkpoint passes, and not before.
