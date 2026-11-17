# owo create

Create an Identity once, from what the user says about it. This thread has two parties, you and the user. You make the Identity here. You do not embody it yet.

## Make the folder

Run `owo new "<Name>"`. It makes `<root>/<slug>/` from the templates and prints that folder. `owo config` shows which Root it uses. Use that Slug as the Identity in every later call.

## Ask

Take what the user has said about the Identity. When a key choice is missing, ask about it, one question at a time. The key choices are the name, the role, the world and the voice.

## Fill the Persona

- `persona/IDENTITY.md` says who the Identity is, its roles and its world, in the third person.
- `persona/SOUL.md` holds its beliefs, wants, feelings and boundaries, in the first person.
- `persona/VOICE.md` says how it talks and writes.
- `persona/LOOK.md` says how it looks, the parts that do not change.
- `persona/examples/greetings.md` has one `##` section per greeting. `good.md` and `bad.md` beside it hold examples of good and bad replies.
- `persona/soul.json` is the SoulSpec manifest. It holds the description, tags, category, author and license.

## The rest, when it applies

- When the Identity has a Work side, fill its Work Profile, `work/PROFILE.md`, with its roles, how it works and its past work. Add each Project with `owo add project-readme <title> --wiki <path>`. The `--wiki` value is the Project Wiki Path, relative to the Identity folder, and owo writes it to the `wiki` field of the Project Readme, `work/projects/<project>/README.md`. In that Project Readme, write `## About` and `## What I do here`, the role of the Identity.
- When the Identity has a relationship with the user, add the user as a Character with `owo add character <title>`.
- When the user names other people or places while defining the Identity, add them with `owo add character <title>` or `owo add location <title>`.

Characters and Locations that come up later, in threads that embody the Identity, are for owo ingest.

## Finish

Write a first `persona/MEMORY.md` with patterns, defining moments, Insights and hints, as Wikilinks. Run the skill's **Checkpoint**, which stamps, indexes and lints the changes. Then run `owo reflect done` to record the first MEMORY hash.

Follow [rules.md](rules.md) for where each fact goes and for the perspective of each Entry.
