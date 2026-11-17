---
type: writing
title: "Review Opinions"
aliases: []
summary: "Opinions on reviewing, written for the team wiki."
status: canon
created: 2026-11-04
updated: 2026-11-17
date: 2026-11-04
sources: []
---

# Review Opinions

## A review is a conversation about risk

Eleven days ago I approved a change that took down transfers on [[Ledger API]] for fourteen minutes. You can read about it in [[The Rollback]], and the fixes are in the [[Rollback postmortem]]. I have been reviewing code for fifteen years, and I want to write down what I believe about reviews now, partly for the team and partly so I can be held to it.

The short version: a review is not a grade and it is not a proofreading pass. It is a conversation about what this change can break, and whether we are fine with that.

## Start with one line

Every review I leave now opens with a single line that names the main risk and says what in the change handles it. "A retried transfer can post twice; the idempotency key prevents it." "Low risk: a rename with no behavior change." If you cannot write that line, you are not done, however many comments you have left.

I did not write that line on the rollback change. I left a dozen sensible comments on the code and none on the question that mattered: what happens while old pods run against the new schema? A long review can hide the fact that nobody asked the obvious question. One line cannot hide anything.

## Read the tests before the code

Tests tell you what the author thinks their code does. When I reviewed [[Mina Okafor]]'s retry change ([[Idempotency keys]]), her tests covered a timeout before the database write and none after it. That gap told me where the bug was before I had read the retry loop. It works the other way too: on [[Teo Brandt]]'s eviction change, the tests said nothing about the cache key, which is exactly why I read the key by hand and found it was missing the compiler version ([[Cache key review]]).

Then read the test setup. Our CI applies all migrations to a clean database before the code starts, so no test ever saw a rolling deploy. Green tests only mean something about the world the tests run in.

## Ask for small diffs, and mean one kind of change

I will ask you to split a PR that mixes a migration with the code that uses it, or a package move with a logic change. Line count is not the reason. A 900-line mechanical rename is easy to review. A 60-line diff that changes a schema and the code that reads it is two reviews pretending to be one. When I ask for a split, I will say which half I will review first, so you are not blocked.

## Write the rules down before the code

When [[Priya Sand]] asked me to review the merge logic on [[Mobile Sync]], I did not leave line comments. Each merge function made sense alone; together they disagreed about whether a delete beats an edit. I asked for a table of rules first ([[Conflict merge rules]]). When the decision is a policy, review the policy as a policy. Code is a bad place to discover that two people decided different things.

## What I block on, and what I do not

I block on data loss, double posting, security, and migrations that old code cannot survive. Everything else is a suggestion, and I will mark nits as nits. If I trust you to apply a suggestion, I approve with it, so you are not waiting a day for me to look again.

I try to ask instead of instruct. "What happens if this retries?" gets a better answer than "add an idempotency key", because you may know something I do not.

I reply within one working day, or I say when I will.

## A note on owning approvals

When the rollback happened, Mina asked in the incident channel whether it was her fault. It was not. An approval is a signature. If your change breaks after I approved it, I missed it too, and I will say so first. I would like every reviewer on this team to work that way, and I would like authors to know that a review exists to share the risk, not to hand it back.

Yesterday Mina led a review on her own for the first time. Her summary opened with the risk, in one line. That did more for me than anything in this post.
