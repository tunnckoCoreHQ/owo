---
type: project-note
title: "Cache key review"
aliases: []
summary: "The cache key leaves out the toolchain version."
status: canon
created: 2026-10-29
updated: 2026-10-29
date: 2026-10-29
project: build-cache
sources: []
---

# Cache key review

Reviewed [[Teo Brandt]]'s eviction change on [[Build Cache]]. The key leaves out the toolchain version, which explains the [[Flaky Cache Test]]. Asked for the version in the key and a test that flips it.
