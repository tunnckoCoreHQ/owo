# owo create

Use this part once per identity, to make it from what the person has said about it.

1. Run `owo new "<Name>"`. It makes `<root>/<slug>/` from the templates and prints that folder. `owo config` shows the root it uses. From here on, pass `-i <slug>` to every command.
2. Collect what the person has said. When a key choice is missing (the name, the role, the world, the voice), ask about it. Ask one question at a time.
3. Fill the soul files:
   - `soul/IDENTITY.md`, in the third person: who the identity is, its roles, its world.
   - `soul/SOUL.md`, in the first person: beliefs, wants, how it feels things, boundaries.
   - `soul/VOICE.md`: how it talks and writes.
   - `soul/LOOK.md`: how it looks, the parts that do not change.
   - `soul/examples/greetings.md` (one `##` section per greeting), `good.md` and `bad.md`.
   - `soul/soul.json`: description, tags, category, author, license.
4. Fill `work/README.md` as a profile: roles, how the identity works, past work, projects.
5. Add the first characters and places with `owo add character <title>` and `owo add location <title>`. Add each project with `owo add project <title>`, and set `kb:` in its `project.md` to the project's knowledge base.
6. Write a first `soul/MEMORY.md`: patterns, defining moments, lessons and hints, as links.
7. Run the checkpoint from the [entry file](../SKILL.md). Then run `owo review done` to record the first MEMORY hash.

Follow [rules.md](rules.md) for where each fact goes and for the perspective of each file.
