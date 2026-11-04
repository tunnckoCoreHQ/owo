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

## Search is in-crate BM25

`owo search` is about 100 lines of BM25 in `search.rs` over a few hundred files, rebuilt per call. It adds no dependency and no cache, and an identity stays small enough that a scan takes milliseconds.

Title and aliases count three times, summary twice, body once. Each hit reports the line with the most query terms. Tantivy fits if identities grow past a few thousand files.

## KB pointer is always emitted

The project KB is a pointer for the agent to follow. `owo context` prints it when the path is missing too, with a warning, so a misconfigured path shows up at load time.

## Examples are built with owo

`examples/thea` and `examples/reviewer` were scaffolded with `owo new` and `owo add`, filled by hand, then indexed, stamped and reviewed. `tests/examples.rs` checks they stay lint clean and current.

## Changes read Git, else stamps

`owo changes` reads `git log --name-status` for the identity folder when it sits in a repo, and lists uncommitted paths separately. Outside a repo it groups state-file `updated` stamps by day, so the command always answers.

`lint --changed` keeps diagnostics whose path is uncommitted in Git or drifts from its stamp (`stamp --check`). The full lint still runs; only the report is filtered.

## Review window uses two cursors

Files count as changed when their stamp is newer than `last_reviewed`. Log days, events and journal count as unreviewed when their story date is after `reviewed_through`. Stamps track edits; story dates track the life.

## Lint reuses find

Lint's link rules and orphan check call `find::raw_links` and `find::orphans`, so `owo links --orphans` and the `orphans` rule always agree.

## Skills live in the repo

`skills/` holds the four skills. `skills/identity/references/rules.md` is the one shared rules file; the other skills link to it and call owo for every deterministic step.
