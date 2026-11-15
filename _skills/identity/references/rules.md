# Identity rules

Shared by the `identity`, `identity-create`, `identity-ingest` and `identity-review` skills. owo does the deterministic work; these rules cover judgment.

## Where each fact lives

Every fact has one home. Write it there once and link to it from everywhere else.

| Fact | Home | Elsewhere |
| :- | :- | :- |
| A small thing today | `life/log/<day>.md`, one bullet (`owo add log`) | |
| A happening worth recalling: a scene, a first, a turning point | `life/events/<slug>.md` (`owo add event --date`) | one bullet in the day's log: `- [[Event]]` |
| A happening that changed the identity | the event, with `defining: true` | MEMORY "Defining moments" at the next review |
| A person, animal, being, other identity | `life/characters/<slug>.md` | links |
| Someone's exact personal identifiers | that character's `## Identifiers` | |
| The identity's own accounts | `identity.json` → `accounts` | IDENTITY.md names the kind |
| A place or room | `life/locations/<place>/overview.md`, rooms beside it | links |
| A promise, plan or running bit still open | `life/threads/<slug>.md` | the Now block lists open threads |
| How the identity talks or writes | `soul/VOICE.md` | |
| A durable value, boundary or belief | `soul/SOUL.md`, through review | `owo soul note` with evidence |
| How the identity looks | `soul/LOOK.md` | |
| Biography, roles, the frame of its world | `soul/IDENTITY.md` | |
| An interpretation of experience | `life/lessons/<slug>.md` with `evidence` | MEMORY links it at review |
| Private feelings about a day | `life/journal/<date>_<slug>.md` | lessons and soul get the abstraction |
| Two sources disagree | `life/conflicts/<slug>.md` | `status: disputed` on affected entries |
| A correction | the owning entry, edited in place | the day's log: `- correction: [[Entry]]` |
| An exact quote worth keeping | the event's `## Lines` | `[[Event#Lines]]` |
| Work on a project | `work/projects/<p>/notes/<date>_<slug>.md` (`owo add note --project <p>`) | |
| Roles, working preferences, past work | `work/README.md` | |
| Facts about a project itself | the project's KB | notes link `[[kb:<p>:<path>#<Section>]]` |
| Long-term memory | `soul/MEMORY.md`, compiled by review | |

## One account per happening

1. A happening lives in its event file when one exists, otherwise in the daily log.
2. A log bullet that points to an event is one line with the link.
3. Exact quotes live in one event's `## Lines`.
4. Indexes, the timeline and the Now block are generated: `owo index`, `owo context`.
5. MEMORY holds patterns, hints and links. Only review writes it.
6. `owo lint --rule dupes` finds copied passages. Keep the passage in its home and turn the copy into a link.

## Perspective

- SOUL.md, MEMORY.md and every life and work entry are first person, in the identity's own voice: characters, places, events, lessons, journal, logs and project notes.
- IDENTITY.md is third person.
- Every character, including the person the identity works with, is a character file like any other.

## Story time

Dates are the dates things happened in the identity's life: `date:` on logs, events and notes. `created` is the day the file was made; `owo stamp` keeps `updated` current.

## Identifiers

Exact personal identifiers (birth dates, handles, emails, phone numbers, addresses, full names of private people) appear once, under `## Identifiers` in that character's file. Everywhere else, refer to them by role and link: "[[Sam]]'s birthday". `owo lint --rule identifiers` reports copies.

## Conflicts

Create `life/conflicts/<slug>.md` (`owo add conflict`) with both accounts, sources and `state: open`, list the affected entries in `affects`, and set `status: disputed` on them. When settled, write the resolution, set `state: resolved`, and return the entries to `status: canon`. Replaced material that still matters keeps `status: superseded` and `superseded_by`.

Source authority, highest first: the person's direct statement or correction, the primary transcript, repeated behaviour across sources, an earlier summary.

## Finishing

Every write session ends with the checkpoint in the [identity skill](../SKILL.md#checkpoint). Fix errors before stopping.
