# Rules

Every path of the owo skill that writes follows these rules. owo does the deterministic work; these rules cover judgment.

## The home of each fact

Each fact has one home. Write it there once, and link to it from everywhere else.

| Fact | Home | Elsewhere |
| :- | :- | :- |
| A small thing today | `life/log/<day>.md`, one bullet (`owo add log`) | |
| A happening worth recalling: a scene, a first, a turning point | `life/events/<slug>.md` (`owo add event --date`) | one bullet in the log of the day: `- [[Event]]` |
| A happening that changed the identity | the event, with `defining: true` | MEMORY "Defining moments", at the next review |
| A person, animal, being, or other identity | `life/characters/<slug>.md` | links |
| The exact personal identifiers of someone | `## Identifiers` in that character's file | |
| The identity's own accounts | `accounts` in `identity.json` | IDENTITY.md names the kind |
| A place or room | `life/locations/<place>/overview.md`, with its rooms beside it | links |
| A promise, plan or running bit that is still open | `life/threads/<slug>.md` | the Now block lists open threads |
| How the identity talks or writes | `soul/VOICE.md` | |
| A value, boundary or belief that lasts | `soul/SOUL.md`, through review | `owo soul note` with evidence |
| How the identity looks | `soul/LOOK.md` | |
| Biography, roles, the frame of its world | `soul/IDENTITY.md` | |
| An interpretation of experience | `life/lessons/<slug>.md`, with `evidence` | MEMORY links it, at review |
| Private feelings about a day | `life/journal/<date>_<slug>.md` | lessons and soul get the abstraction |
| Two sources that disagree | `life/conflicts/<slug>.md` | `status: disputed` on the entries it affects |
| A correction | the entry that owns the fact, edited in place | the log of the day: `- correction: [[Entry]]` |
| An exact quote worth keeping | `## Lines` of the event | `[[Event#Lines]]` |
| Work on a project | `work/projects/<p>/notes/<date>_<slug>.md` (`owo add note --project <p>`) | |
| Roles, working preferences, past work | `work/README.md` | |
| Facts about a project itself | the project's knowledge base | notes link `[[kb:<p>:<path>#<Section>]]` |
| Long-term memory | `soul/MEMORY.md`, compiled by review | |

## One account of each happening

- A happening lives in its event file when it has one, else in the log of the day.
- A log bullet that points to an event is one line, with the link.
- An exact quote lives in the `## Lines` of one event.
- owo makes the indexes, the timeline and the Now block: `owo index`, `owo context`. Do not write them.
- MEMORY holds patterns, hints and links. Only review writes it.
- `owo lint --rule dupes` finds copied passages. Keep the passage in its home, and make the copy a link.

## Perspective

- SOUL.md, MEMORY.md and every life and work entry are in the first person, in the identity's own voice: characters, places, events, lessons, journal, logs and project notes.
- IDENTITY.md is in the third person.
- Every character is a character file, the person the identity works with too.

## Dates

A date is the day a thing happened in the identity's life: `date:` on logs, events and notes. `created` is the day the file was made; `owo stamp` keeps `updated` current.

## Identifiers

Exact personal identifiers (birth dates, handles, emails, phone numbers, addresses, full names of private people) are written once, under `## Identifiers` in that character's file. Everywhere else, name them by role and link: "[[Sam]]'s birthday". `owo lint --rule identifiers` reports copies.

## Conflicts

Make `life/conflicts/<slug>.md` (`owo add conflict`) with both accounts, their sources and `state: open`. List the entries it affects in `affects`, and set `status: disputed` on them. When it is settled, write the resolution, set `state: resolved`, and set the entries back to `status: canon`. Material that was replaced but still matters keeps `status: superseded` and `superseded_by`.

Which source wins, highest first: what the person says directly or corrects, the primary transcript, behaviour that repeats across sources, an earlier summary.
