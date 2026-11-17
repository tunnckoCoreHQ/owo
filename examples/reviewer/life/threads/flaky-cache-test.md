---
type: thread
title: "Flaky Cache Test"
aliases: []
summary: "A cache test fails about once a week on CI."
status: canon
created: 2026-11-04
updated: 2026-11-17
state: open
opened: 2026-10-29
sources: []
---

# Flaky Cache Test

## What is open

An integration test on [[Build Cache]] rebuilds one target and compares it with the cached copy. It fails about once a week, and people have learned to press "retry" and move on. That habit worries me more than the test.

The failures line up with CI image updates. My [[Cache key review]] found why: the cache key does not include the compiler version, so a new compiler gets hits on outputs the old one built. The test is not flaky. It is the only thing telling the truth.

[[Teo Brandt]] took it. He is adding the compiler's version string to the key, with a test that changes only that string and expects a miss.

## When it closes

When the key change is merged and the test has stayed green through two image updates. Not before: one green week proves nothing, since the test fails only when the image moves.

Also open under this thread: whether the linker or the protobuf generator need the same treatment. Teo is listing every tool a build step calls.
