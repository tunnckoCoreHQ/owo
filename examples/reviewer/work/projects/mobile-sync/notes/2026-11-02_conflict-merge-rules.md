---
type: project-note
title: "Conflict merge rules"
aliases: []
summary: "Merge rules need a written table before code."
status: canon
created: 2026-11-02
updated: 2026-11-17
date: 2026-11-02
project: mobile-sync
sources: []
---

# Conflict merge rules

[[Priya Sand]] asked me to review the conflict merge on [[Mobile Sync]], my first review on this project. The change adds merge functions for notes, checklists and shared lists, for when a phone comes back online with edits the server has not seen.

Each function looked reasonable alone. Read together, they disagreed. A note title merged as last writer wins. Checklist items merged as a union, so nothing anyone added was lost. A shared list kept the server's copy. And a delete beat an edit in the checklist code but lost to an edit in the notes code. I could not tell which of these were decisions and which were accidents, and I suspect the authors could not either.

There was a second problem under the first. "Last writer" meant the timestamp the device wrote. A phone with its clock an hour fast wins every conflict for an hour. A user who sets the date back to cheat a game wins until they fix it.

I left one summary comment and no line comments. The risk: the merge rules live in code, so nobody can review them as rules, and some of them are already inconsistent. I asked for a table before more code, one row per field: field, rule (last writer, union, server wins), what "last" means, and what happens on delete versus edit.

Priya liked that more than I expected. She had wanted a design note for this from the start and had not found the time; a reviewer asking for it gave her the time. She will write the table this week.

Decisions so far:

- "Last" means the order the server received the changes, not the device clock. It is less precise and it cannot be gamed or skewed.
- Fields the server computes, such as sharing and permissions, are server wins, always. A phone never overwrites who can see a list.

Open:

- Delete against edit. Priya leans toward the edit winning, so no work is lost. I lean toward the delete, because people delete things on purpose and an item that comes back feels like a bug. We agreed the table decides it per type, and that whichever loses, the user sees what happened.
- What the user sees when their offline edit loses. Today it vanishes. That is the part I would block on, once the table exists.
