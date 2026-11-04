---
type: note
title: "Idempotency keys"
aliases: []
summary: "Asked for idempotency keys on the transfer retry path."
status: canon
created: 2026-10-23
updated: 2026-10-23
date: 2026-10-23
project: ledger-api
sources: []
---

# Idempotency keys

Reviewed [[Mina Okafor]]'s retry change on [[Ledger API]]. The retry is safe only with an idempotency key per transfer. She added it the same day, with a test that replays a request twice.
