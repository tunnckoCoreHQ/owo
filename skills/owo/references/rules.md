# Rules

Every path of the owo skill that writes follows these rules. owo does the deterministic work; these rules cover judgment.

## The home of each fact

Each fact has one home. Write it there once, and link to it from everywhere else with a Wikilink.

| Fact | Home | Elsewhere |
| :- | :- | :- |
| A small thing today | `life/daily-log/<day>_<slug>.md`, one bullet in the Daily Log (`owo add daily-log <title>`) | |
| A happening worth recalling: a scene, a first, a turning point | an Event, `life/events/<slug>.md` (`owo add event <title> --date <day>`) | one bullet in the Daily Log of the day: `- [[Event]]` |
| A happening that changed the Identity | the Event, with `defining: true` | MEMORY "Defining moments", at the next Reflection |
| A person, animal, being, or other Identity | a Character, `life/characters/<slug>.md` | Wikilinks |
| The Identifiers of someone | `## Identifiers` in the Entry of that Character | |
| The Identity's own accounts | `accounts` in `identity.json` | IDENTITY.md says what sort of account each is |
| A place or room | a Location, `life/locations/<place>/overview.md`, with its location details beside it | Wikilinks |
| A promise, plan or running bit that is still open | a Thread, `life/threads/<slug>.md` | the Current State lists open Threads |
| How the Identity talks or writes | `persona/VOICE.md` | |
| A value, boundary or belief that lasts | `persona/SOUL.md`, through a Reflection | a Persona Change with Evidence (`owo persona change`) |
| How the Identity looks | `persona/LOOK.md` | |
| Biography, roles, the frame of its world | `persona/IDENTITY.md` | |
| A life lesson that the experience really taught | an Insight, `life/insights/<slug>.md`, with `evidence` | MEMORY links it, at a Reflection |
| Private feelings about a day | a Journal, `life/journal/<date>_<slug>.md` | Insights and the Soul get the abstraction |
| Two accounts that both still stand, with nothing to decide between them | a Conflict, `life/conflicts/<slug>.md` | `status: disputed` on the Entries it affects |
| A correction | the Entry that owns the fact, edited in place | the Daily Log of the day: `- correction: [[Entry]]` |
| An exact quote worth keeping | `## Lines` of the Event | `[[Event#Lines]]` |
| Work on a Project | a Project Note, `work/projects/<p>/notes/<date>_<slug>.md` (`owo add project-note --project <p>`) | |
| Roles, working preferences, past work | the Work Profile, `work/PROFILE.md` | |
| Facts about a Project itself | its Project Wiki | Project Notes link `[[wiki:<p>:<path>#<Section>]]` |
| Long-term memory | `persona/MEMORY.md`, compiled by a Reflection | |

## One account of each happening

- A happening lives in its Event when it has one, else in the Daily Log of the day.
- A Daily Log bullet that points to an Event is one line, with the Wikilink.
- An exact quote lives in the `## Lines` of one Event.
- owo builds the Generated Files (each Index and the Timeline) and the Current State: `owo index`, `owo context`. Leave them to owo.
- MEMORY holds patterns, hints and Wikilinks. Create writes the first one, and after that only a Reflection writes it.
- `owo lint --rule dupes` finds copied passages. Keep the passage in its home, and make the copy a Wikilink.

## Citing Raw Material

Cite Raw Material only in the `sources` list in the frontmatter of the Entry, as its path from the `sources/` folder of the Identity: `<file>.md#L<a>-L<b>`. The body of an Entry, a Conflict too, never holds a path of Raw Material, such as the path of a Slice.

## Perspective

- SOUL.md, MEMORY.md and every Life and Work Entry are in the first person, in the Identity's own voice: Characters, Locations, Events, Insights, Journals, Daily Logs and Project Notes.
- IDENTITY.md is in the third person.
- Every Character has its own Entry, the person the Identity works with too.

## Dates

A date is a story date, the day a thing happened in Story Time: `date:` on Daily Logs, Events and Project Notes. `created` is the day the file was made; `owo stamp` keeps `updated` current.

## Identifiers

Identifiers (birth dates, handles, emails, phone numbers, addresses, full names of private people) are written once, under `## Identifiers` in the Entry of that Character. Everywhere else, name them by role and link: "[[Sam]]'s birthday". `owo lint --rule identifiers` reports copies.

## Conflicts

Make `life/conflicts/<slug>.md` (`owo add conflict <title>`) with both accounts in the body, their Raw Material in `sources`, and `state: open`. List the Entries it affects in `affects`, and set `status: disputed` on them. When it is settled, write the resolution, set `state: resolved`, and set the Entries back to `status: canon`. Material that was replaced but still matters keeps `status: superseded` and `superseded_by`.

Which Raw Material wins, highest first: what the person says directly or corrects, the primary Transcript, behaviour that repeats across Raw Material, an earlier recap.
