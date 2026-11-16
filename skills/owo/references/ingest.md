# owo ingest

Put Raw Material into the Identity. Raw Material is a Transcript, notes, a document, or the Slice of a Sync Run.

## The Raw Material

- A Sync Run gives you an Ingest Prompt and a Slice in `<root>/sources/<slug>/`. The Ingest Prompt is the whole task. Do every step of it, the last one too, which is `owo sync done` for that run.
- A `sync_push` result that is Due has the Slice in `slice_text` and the Ingest Prompt in `prompt_text`, for an agent that cannot read the files. Its `slice` field is the path of the Slice from the Root. Cite it without the leading `sources/`.
- Copy any other file into `<root>/sources/<slug>/` first, and cite it by its path relative to `sources/`.

## Read

Run `owo context` and `owo state` first, so you know the Identity and its Current State. Then read the Raw Material, and list its durable facts. These are happenings, people, places, decisions, quotes, work done and promises still open. Leave out small talk and facts the Identity has already.

## Record each fact

- Find the home of the fact with `owo resolve <name>` or `owo search <words>`. [rules.md](rules.md) has the table of homes.
- When the home exists, read it with `owo show`, and merge the fact in place.
- When it does not exist, make it with `owo add <kind> <title> [--date <day>] [--project <p>]`, and fill the template.
- Add the Raw Material to the `sources` list in the frontmatter of the Entry, with the lines it came from: `<slug>/<file>.md#L<a>-L<b>`.
- Put a small thing as one bullet in the Daily Log of the day it happened. Run `owo add daily-log --date <day>`, which makes the Daily Log or prints `exists <path>` when it is there already. Then add the bullet to that file. An Event gets one bullet in that Daily Log, with a Wikilink to it.
- When the Raw Material corrects or settles an Entry, for example a later finding that answers an open guess, update the Entry and cite the new Raw Material.
- When two accounts both still stand and nothing decides between them, record a Conflict with `owo add conflict <title>`, and keep the Entry as it is.
- When the Raw Material shows that a Thread is done, set `state: closed` on it and cite the Raw Material.
- The Persona Files, MEMORY included, change only on the [reflect path](reflect.md).

## Finish

Run the skill's **Checkpoint**, which stamps, indexes and lints the changes. Report the paths that `owo stamp` lists, or "no changes". For a Sync Run, run its `owo sync done` command after the Checkpoint passes, and not before.
