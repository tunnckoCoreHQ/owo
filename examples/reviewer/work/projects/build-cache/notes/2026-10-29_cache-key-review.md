---
type: project-note
title: "Cache key review"
aliases: []
summary: "The cache key leaves out the toolchain version."
status: canon
created: 2026-10-29
updated: 2026-11-17
date: 2026-10-29
project: build-cache
sources: []
---

# Cache key review

[[Teo Brandt]]'s eviction change on [[Build Cache]]: about 1,400 lines. It replaces plain LRU with size-aware eviction, so one huge test bundle can no longer push out hundreds of small compile outputs. It also moves the key code into a new package, and that move is most of the line count.

I read the eviction design yesterday, so I went straight to the tests. They test eviction well. They test the key not at all, and the key code changed, so I read the key code by hand.

The key hashes the source files of the step, the build flags, the lockfile and the target platform. It does not hash the compiler version. The compiler comes from whatever the CI image puts on `PATH`. When the image updates, the new compiler gets cache hits on outputs that the old compiler built. Most of the time those outputs are close enough that nobody notices. The integration test that rebuilds one target and compares it with the cached copy notices, and that is the test that goes red about once a week. I opened the [[Flaky Cache Test]] thread for it.

What I asked for:

- The compiler's full version string in the key.
- A test that changes only the version string and asserts a miss.
- The package move as its own PR, so the eviction logic gets a review that is about eviction.

Teo agreed to the first two at once. He pushed back on one idea I floated, hashing the whole CI image digest, and he was right: the digest changes for every unrelated package update, so the cache would go cold every week. We settled on the compiler version only. The split he did not love, but he said he would do it for the follow-up.

I did not approve the eviction change today. I want to read it without a package move around it.

Open: the compiler is not the only tool in the image that shapes outputs. The linker and the protobuf generator do too. I do not know yet whether either has bitten us. Teo will list every tool a build step calls, and we decide from that list, not from my guess.
