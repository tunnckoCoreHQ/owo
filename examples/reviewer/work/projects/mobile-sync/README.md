---
type: project-readme
title: "Mobile Sync"
aliases: []
summary: "Offline sync for the mobile app; I review merge logic."
status: canon
created: 2026-11-04
updated: 2026-11-17
wiki: "../wiki/mobile-sync/lat.md"
repo: "example/mobile-sync"
started: 2026-11-02
sources: []
---

# Mobile Sync

## About

Mobile Sync lets the mobile app work offline and merges the changes a phone made while offline when it reconnects. It decides, field by field, whose edit survives when two devices changed the same record.

## What I do here

I review the conflict merge rules and the sync protocol. I am new here, and the team is not new to the problem, so I ask more than I block.

[[Priya Sand]] leads the project and wants a written design note before code, which suits me. My first review, the [[Conflict merge rules]], ended in a request for a table of rules instead of more code.
