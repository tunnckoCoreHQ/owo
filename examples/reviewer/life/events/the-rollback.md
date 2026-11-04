---
type: event
title: "The Rollback"
aliases: []
summary: "A schema migration shipped with the code that needed it and had to be rolled back."
status: canon
created: 2026-11-04
updated: 2026-11-04
date: 2026-10-24
places: []
people: ["[[Mina Okafor]]"]
defining: true
sources: []
---

# The Rollback

## What happened

A ledger release ran a column rename in the same deploy as the code reading it. Old pods crashed, and the release was rolled back within twenty minutes. I had approved it.

## Key facts

- Postmortem: [[Rollback postmortem]].
