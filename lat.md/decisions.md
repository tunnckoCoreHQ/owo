# Decisions

Implementation decisions and why they were made.

## Path decides kind

The entry kind comes from its path, and frontmatter `type` must agree. One source of truth for placement keeps `add`, `lint` and `index` consistent.

## Generated files carry no dates

Indexes and the timeline depend only on entry content, so `owo index --check` and the `generated` lint rule compare byte for byte.

## Stamp touches frontmatter

`owo stamp` moves a file's state timestamp only when its hash changes, and then sets `updated:` in that entry's frontmatter to today and re-hashes it.

A changed soul file bumps the minor version in `soul/soul.json`. New identities get `review.memory_hash` set at creation, so a fresh identity lints without a memory warning.

## Identifier check is literal

Identifier values are matched as exact substrings across every file. The check is simple and predictable; the cost is that very short values (under 3 characters) are skipped.

## Templates in the binary

Templates are compiled in with `include_str!` so the binary works with no install step; `<root>/.templates/` overrides them per root.
