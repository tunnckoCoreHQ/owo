# owo create

Create an identity once, from what the user says about it. This thread has two parties, you and the user. You make the identity here. You do not embody it yet.

## Make the folder

Run `owo new "<Name>"`. It makes `<root>/<slug>/` from the templates and prints that folder. `owo config` shows which root it uses. Use that slug as the identity in every later call.

## Ask

Take what the user has said about the identity. When a key choice is missing, ask about it, one question at a time. The key choices are the name, the role, the world and the voice.

## Fill the soul

- `soul/IDENTITY.md` says who the identity is, its roles and its world, in the third person.
- `soul/SOUL.md` holds its beliefs, wants, feelings and boundaries, in the first person.
- `soul/VOICE.md` says how it talks and writes.
- `soul/LOOK.md` says how it looks, the parts that do not change.
- `soul/examples/greetings.md` has one `##` section per greeting. `good.md` and `bad.md` beside it hold examples of good and bad replies.
- `soul/soul.json` holds the description, tags, category, author and license.

## The rest, when it applies

- When the identity has a work side, fill `work/README.md` with its roles, how it works, its past work and its projects. Add each project with `owo add project <title>`, and set `kb:` in its `project.md` to the knowledge base of the project.
- When the identity has a relationship with the user, add the user as a character with `owo add character <title>`.
- When the user names other people or places while defining the identity, add them with `owo add character <title>` or `owo add location <title>`.

Characters and places that come up later, in threads that embody the identity, are for owo ingest.

## Finish

Write a first `soul/MEMORY.md` with patterns, defining moments, lessons and hints, as links. Run the skill's **Checkpoint**, which stamps, indexes and lints the changes. Then run `owo review done` to record the first MEMORY hash.

Follow [rules.md](rules.md) for where each fact goes and for the perspective of each file.
