# Lint

`owo lint` checks one identity. Errors fail the run with exit 1; warnings and infos report only.

The schema rule requires `aliases` and `sources` to be lists of strings. Missing fields and null values fail; an empty list is valid. Outside Git, changed-file lint compares each stamp's local day with today.

## Rules

Each rule has a code usable with `--rule`.

- `tree` (error, a `.gitignore` without `sessions/` warns): required files and folders; `identity.json` schema `owo/1` and slug; SoulSpec required fields; location folders have `overview.md`, slug names, one level deep; files outside the layout; Markdown files that are not valid UTF-8 (owo reads them with U+FFFD in place of the bad bytes, and `owo stamp` refuses to run until they are fixed). It warns when the `.gitignore` of the identity folder has no `sessions/` line; see [[decisions#Sources and sessions live in the identity folder]].
- `schema` (error, empty summary warns): frontmatter per [[formats#Entry frontmatter]]. A day has one daily log: a second daily log with the same `date` is an error that names the first.
- `persona-plain` (error): persona files have no frontmatter.
- `links` (error): every wikilink in body and frontmatter resolves; ambiguous wikilinks warn, and so does a `#Section` the target lacks. `wiki:` wikilinks belong to the `wiki` rule.
- `unique` (error): titles and aliases are unique across the identity.
- `identifiers` (error): values under any `## Identifiers` bullet (text after the first colon, 3+ characters) appear only in that section, and `identity.json` account handles only in `identity.json`. Headings inside fenced code do not count.
- `journal` (error): only journal entries link into `life/journal/`.
- `supersede` (error): `superseded` entries have a `superseded_by` that resolves to one entry; `disputed` entries are linked from an open conflict.
- `generated` (error): generated files match `owo index`.
- `memory-reflection` (warning): MEMORY.md hash equals `reflection.memory_hash`.
- `state` (warning): state files match content hashes; it reports what `owo stamp --check` reports, and a state file that does not parse.
- `budget` (warning): persona file word counts within soft budgets (IDENTITY 800, SOUL 800, VOICE 1000, LOOK 600, MEMORY 1500), overridable in `identity.json`.
- `dupes` (warning): shared 12-word passages between authored files; the message quotes one shared passage.
- `orphans` (warning): entries with zero inbound wikilinks; daily logs, journal, project notes and persona files are exempt.
- `persona-evidence` (warning): each persona change cites two wikilinks that each resolve to one entry, or one `defining: true` event.
- `reflection-due` (info): never reflected, or `reflect_every_days` passed.
- `wiki` (error for an unknown project or malformed link; warning for a missing file or section): `[[wiki:<p>:<path>#<Section>]]` wikilinks. A project wiki that is not on disk is skipped, so identities lint clean on machines without the project checked out.
