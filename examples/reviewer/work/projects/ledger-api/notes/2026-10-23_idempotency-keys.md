---
type: project-note
title: "Idempotency keys"
aliases: []
summary: "Asked for idempotency keys on the transfer retry path."
status: canon
created: 2026-10-23
updated: 2026-11-17
date: 2026-10-23
project: ledger-api
sources: []
---

# Idempotency keys

[[Mina Okafor]] sent a change that retries `POST /transfers` on [[Ledger API]] when the call times out: three attempts, exponential backoff, jitter. The code was clean. The description said "makes transfers resilient to timeouts".

I read the tests first. They covered a timeout before the request reached the database, and nothing else. That told me what she thought a timeout was. The case that matters is the other one: the database commits the transfer, the response is lost on the way back, and the client retries. With this change, that posts the transfer twice. Two balanced entry pairs, both valid, one of them wrong.

My summary comment was one line of risk and one question: "A retried transfer can post twice. What makes the second attempt a no-op?"

She came back after lunch with the answer. The caller now sends an `Idempotency-Key` header, one per logical transfer. The ledger stores it in a new `idempotency_key` column with a unique index on `(account_id, idempotency_key)`. A second request with the same key returns the stored response and writes nothing. Her new test sends the same request twice and asserts one entry pair and two identical responses. I approved it that afternoon.

Decisions, and why:

The key comes from the caller, not from the ledger. Only the caller knows that two requests are the same transfer.

The unique index is the guard, not a lookup before the insert. A lookup has a race: two attempts in flight both see "no key yet" and both insert. The index makes the database refuse the second one, and the handler turns that refusal into the stored response.

Still open:

- There is no test for two attempts in flight at once. The index covers it, but nothing proves the handler catches the unique violation and returns the first response instead of a 500. Mina took this as a follow-up.
- Nobody has decided how long a key lives. Forever is simple and the table grows forever. I asked her to bring a number with a reason to our next weekly hour.

Background on the retry policy: [[wiki:ledger-api:lat.md#Retries]].
