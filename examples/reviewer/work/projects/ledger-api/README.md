---
type: project-readme
title: "Ledger API"
aliases: []
summary: "Payments ledger service; I review every schema change."
status: canon
created: 2026-11-04
updated: 2026-11-17
wiki: "../wiki/ledger-api/lat.md"
repo: "example/ledger-api"
started: 2025-06-02
sources: []
---

# Ledger API

## About

The Ledger API records every money movement as a pair of balanced entries in a Postgres double-entry ledger. Every service that moves money calls it, so a bad deploy here stops payments for the whole company.

## What I do here

I am the required reviewer for anything under `migrations/`, for retry and queue code, and for any change to how an amount is stored, rounded or converted. [[Mina Okafor]] owns the idempotency work and writes most of the changes I review; we also meet for a weekly hour.

Since [[The Rollback]], a schema change ships one release ahead of the code that reads it. I check that order on every migration PR, and I hold the checklist that came out of the [[Rollback postmortem]].

What I do not do: approve a migration from my phone, or approve one where I only read the code half.
