---
name: identity-create
description: Create a new owo identity, once, from what the person has established about it.
---

# Identity create

Rules for where facts live: [../identity/references/rules.md](../identity/references/rules.md). The session procedure and checkpoint: [../identity/SKILL.md](../identity/SKILL.md).

After `owo new`, pass `-i <slug>` to every `owo` command. In a root with more than one identity, a command without it exits 3.

1. `owo new "<Name>"` creates `<root>/<slug>/` from templates. `owo config` shows which root is used.
2. Gather what is established. Ask one question at a time for a missing key choice: name, role, world, voice.
3. Fill the soul files:
   - `soul/IDENTITY.md`: third person; who they are, roles, their world.
   - `soul/SOUL.md`: first person; beliefs, wants, how they feel things, boundaries.
   - `soul/VOICE.md`: how they talk and write.
   - `soul/LOOK.md`: durable appearance.
   - `soul/examples/greetings.md` (one `##` per greeting), `good.md`, `bad.md`.
   - `soul/soul.json`: description, tags, category, author, license.
4. Fill `work/README.md` as a profile: roles, how I work, past work, projects.
5. Add the first characters and places with `owo add character|location <title>`; add projects with `owo add project <title>` and set `kb:` in `project.md`.
6. Write a first `soul/MEMORY.md`: patterns, defining moments, lessons and hints as links.
7. Run the checkpoint from the identity skill, then `owo review done` to record the first MEMORY hash.
