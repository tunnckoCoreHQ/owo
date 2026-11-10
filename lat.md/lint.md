# Lint

`owo lint` checks one identity. Errors fail the run with exit 1; warnings and infos report only.

## Rules

Each rule has a code usable with `--rule`.

- `tree` (error): required files and folders; `identity.json` schema `owo/1` and slug; SoulSpec required fields; location folders have `overview.md`, slug names, one level deep; files outside the layout; Markdown files that are not valid UTF-8 (owo reads them with U+FFFD in place of the bad bytes, and `owo stamp` refuses to run until they are fixed).
- `schema` (error, empty summary warns): frontmatter per [[formats#Entry frontmatter]].
- `soul-plain` (error): soul Markdown has no frontmatter.
- `links` (error): every wiki-link in body and frontmatter resolves; ambiguous links warn, and so does a `#Section` the target lacks. `kb:` links belong to the `kb` rule.
- `unique` (error): titles and aliases are unique across the identity.
- `identifiers` (error): values under any `## Identifiers` bullet (text after the first colon, 3+ characters) appear only in that section, and `identity.json` account handles only in `identity.json`. Headings inside fenced code do not count.
- `journal` (error): only journal entries link into `life/journal/`.
- `supersede` (error): `superseded` entries have a resolving `superseded_by`; `disputed` entries are linked from an open conflict.
- `generated` (error): generated files match `owo index`.
- `memory-review` (warning): MEMORY.md hash equals `review.memory_hash`.
- `state` (warning): state files match content hashes.
- `budget` (warning): soul word counts within soft budgets (IDENTITY 800, SOUL 800, VOICE 1000, LOOK 600, MEMORY 1500), overridable in `identity.json`.
- `dupes` (warning): shared 12-word passages between authored files; the message quotes one shared passage.
- `orphans` (warning): entries with zero inbound links; logs, journal, notes and soul files are exempt.
- `soul-evidence` (warning): each soul history record cites two resolving links or one `defining: true` event.
- `review-due` (info): never reviewed, or `review_every_days` passed.
- `kb` (error for an unknown project or malformed link; warning for a missing file or section): `[[kb:<p>:<path>#<Section>]]` links. A KB folder that is not on disk is skipped, so identities lint clean on machines without the project checked out.
