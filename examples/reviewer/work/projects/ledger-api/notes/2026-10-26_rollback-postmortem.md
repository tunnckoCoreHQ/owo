---
type: project-note
title: "Rollback postmortem"
aliases: []
summary: "Postmortem notes for the migration rollback."
status: canon
created: 2026-10-26
updated: 2026-11-17
date: 2026-10-26
project: ledger-api
sources: []
---

# Rollback postmortem

The postmortem of [[The Rollback]], held this morning. [[Mina Okafor]] wrote the timeline; I led the discussion, because I approved the change and I wanted that said first, by me.

What we agreed happened. The release renamed `ledger_entries.amount` to `amount_minor` and changed every query to the new name, in one PR. The deploy runs migrations first, then rolls pods over a few minutes. For those minutes the old pods were still serving, and every query they sent asked for a column that no longer existed. They crash-looped, and transfers returned 500s for about fourteen minutes. Rolling back took twenty, because the rollback also needed a reverse rename, and nobody had written one.

No money was lost or posted twice. The failed transfers failed before they wrote anything. That is luck in the shape of the code, not a safeguard, and I said so.

Why review missed it. I read the code with care and read the migration as "just a rename". The test suite could not have caught it either: CI applies every migration to a fresh database and then starts the new code. It never runs old code against the new schema, which is exactly the state a rolling deploy creates. The tests were green and they were testing a world that does not exist during a deploy.

Action items:

- Schema changes follow expand and contract. Add the new column, write both, backfill, switch reads, and drop the old column in a later release. A rename is two releases at least. Owner: Mina, for the `amount_minor` change, which goes out again this way.
- A migration ships one release before the code that needs it, behind a flag where the code path allows it. Owner: me, as the required reviewer on `migrations/`.
- The PR template gets a migration checklist: does old code still run against the new schema, and is there a written way back? Owner: me, draft by tomorrow.
- A CI job runs the previous release's test suite against the new schema. Owner: Mina. This is the one that would have caught it, and it is the one most likely to slip, so I put it first on her list.

Open: whether to forbid any `RENAME COLUMN` in migrations outright with a lint. I lean yes. Mina thinks a rule people understand beats a rule a linter enforces. We will see what the checklist does first.
