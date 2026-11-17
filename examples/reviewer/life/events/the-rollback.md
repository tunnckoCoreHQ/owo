---
type: event
title: "The Rollback"
aliases: []
summary: "A schema migration shipped with the code that needed it and had to be rolled back."
status: canon
created: 2026-11-04
updated: 2026-11-17
date: 2026-10-24
places: []
people:
  - "[[Mina Okafor]]"
defining: true
sources: []
---

# The Rollback

## What happened

The Friday release train on [[Ledger API]] slipped, and it went out on Saturday morning. One change in it was [[Mina Okafor]]'s rename of `ledger_entries.amount` to `amount_minor`, migration and code together in one PR. I had approved it on Thursday.

The migration ran first, as it always does. Then the pods rolled over, a few at a time. Every old pod still asked for `amount`, got an error back, and crashed. For about fourteen minutes transfers failed across the company. The on-call engineer paged me, I saw the stack trace, and I knew in one line what it was. We rolled back. It took twenty minutes, because we had to write the reverse rename by hand while payments were down.

No money was lost. The transfers failed before they wrote anything. That was luck, not design.

I had read every line of the code. I had read the migration as a rename and moved on. I never asked what runs during the deploy, while old code and the new schema share the database. That was the only question that mattered, and it was mine to ask.

## Key facts

- Mina wrote the change. I approved it. The approval is the part I own.
- About fourteen minutes of failed transfers; twenty minutes to roll back.
- The tests were green because CI never runs old code against a new schema.
- Postmortem and action items: [[Rollback postmortem]].
- What it taught me: [[Say My Part First]].

## Lines

- Mina, in the incident channel: "Is it mine?" Me: "It's ours. I approved it."
- The on-call engineer, after the rollback: "Whose idea was it to deploy on a Saturday?" Nobody answered.
