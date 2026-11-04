# Lint

`owo lint` checks one identity. Errors fail the run with exit 1; warnings and infos report only.

## Rules

Each rule has a code usable with `--rule`.

- `tree` (error): required files and folders; `identity.json` schema `owo/1` and slug; SoulSpec required fields; location folders have `overview.md`, slug names, one level deep; files outside the layout.
- `schema` (error, empty summary warns): frontmatter per [[formats#Entry frontmatter]].
- `soul-plain` (error): soul Markdown has no frontmatter.
- `links` (error): every wiki-link in body and frontmatter resolves; ambiguous links warn. `kb:` links are skipped for now.
- `unique` (error): titles and aliases are unique across the identity.
- `identifiers` (error): values under any `## Identifiers` bullet (text after the first colon, 3+ characters) and `identity.json` account handles appear only in their home file.
- `journal` (error): only journal entries link into `life/journal/`.
- `supersede` (error): `superseded` entries have a resolving `superseded_by`; `disputed` entries are linked from an open conflict.
- `generated` (error): generated files match `owo index`.
- `memory-review` (warning): MEMORY.md hash equals `review.memory_hash`.
- `state` (warning): state files match content hashes.
- `budget` (warning): soul word counts within soft budgets (IDENTITY 800, SOUL 800, VOICE 1000, LOOK 600, MEMORY 1500), overridable in `identity.json`.
- `dupes` (warning): shared 12-word passages between authored files.
- `orphans` (warning): entries with zero inbound links; logs, journal, notes and soul files are exempt.
- `soul-evidence` (warning): each soul history record cites two resolving links or one `defining: true` event.
- `review-due` (info): never reviewed, or `review_every_days` passed.
