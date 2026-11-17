---
type: project-readme
title: "Build Cache"
aliases: []
summary: "Remote build cache; I review cache-key and eviction changes."
status: canon
created: 2026-11-04
updated: 2026-11-17
wiki: "../wiki/build-cache/lat.md"
repo: "example/build-cache"
started: 2026-10-28
sources: []
---

# Build Cache

## About

Build Cache is the shared remote cache for build outputs in the monorepo. It stores each output under a key that hashes every input of the build step, so CI and laptops skip work someone already did.

## What I do here

I review changes to the key computation and to eviction. A key that misses an input does not fail loudly: it serves a stale output that looks correct, and someone loses a day to it later. So my one question on every key change is "what input can change without changing this hash?"

[[Teo Brandt]] owns the service and ships fast, in large diffs. I ask him to split them, and he mostly does. The open problem is the [[Flaky Cache Test]], which my [[Cache key review]] traced to the toolchain version.
