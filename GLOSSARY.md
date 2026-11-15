# owo

owo keeps a living agent identity as a folder of plain files. Hosts load the identity, record their sessions into it and review it, so one identity lasts across hosts and projects.

## Language

### Identity

**Root**:
The folder that holds every identity, the sources and the session data.
_Avoid_: identities root, home, workspace

**Identity**:
One persona that an agent takes on, with its own soul, life and work. It is one folder in the root.
_Avoid_: agent, persona, character, profile

**Slug**:
The lowercase, hyphenated form of a name. It names an identity, a project and an entry on disk.
_Avoid_: id, key, handle

**Scope**:
One of the three parts of an identity: soul, life or work.
_Avoid_: part, side, section

**Soul**:
The scope that says who the identity is. It changes slowly, and only through a review, with evidence.
_Avoid_: personality, persona, core

**Life**:
The scope that holds what the identity lived and knows, written in its own voice.
_Avoid_: world, lore, memory

**Work**:
The scope that holds the professional side of the identity: its profile and its own experience of each project.
_Avoid_: job, career

**Soul file**:
One of the five entries of the soul: IDENTITY, SOUL, VOICE, LOOK and MEMORY.
_Avoid_: soul entry, core file

**MEMORY**:
The soul file that holds the compiled long-term memory of the identity. Only a review writes it.
_Avoid_: memories, memory file

### Entries

**Entry**:
One Markdown file that a person or an agent writes in an identity. A soul file, a character and a project note are all entries.
_Avoid_: document, page, record, item

**Kind**:
What an entry is, such as character, event, note or soul. The path of the entry decides it.
_Avoid_: type, category, class

**Type**:
The kind that a life or work entry declares in its frontmatter. It must equal the kind that the path gives.
_Avoid_: kind, category

**Category**:
A folder of life that holds the entries of one kind, such as characters or events.
_Avoid_: collection, section, type

**Status**:
How far an entry stands as fact: canon, disputed or superseded.
_Avoid_: state

**Summary**:
The one-line description in the frontmatter of an entry. Indexes, search and the Now block show it.
_Avoid_: description, abstract

**Link**:
A wiki-link from one entry to another, by title, alias or slug.
_Avoid_: reference, pointer, wikilink

**Source**:
A raw file outside every identity that entries cite: a transcript, a note or a slice.
_Avoid_: reference, evidence, raw material

**Evidence**:
The links that support a lesson or a soul change.
_Avoid_: source, proof, citation

**Identifier**:
An exact personal value of a character, such as a birth date, a handle or an address. It is written only in the entry of that character.
_Avoid_: PII, id, personal data

**Generated file**:
A file that owo builds from entry content and that no person or agent edits: an index or the timeline.
_Avoid_: derived file, cache, output

**Index**:
The generated list of the entries of one category or one scope, with their summaries.
_Avoid_: table of contents, catalog

**Timeline**:
The generated list of events in story time, grouped under eras.
_Avoid_: history, chronology

### Kinds of entry

**Character**:
A person, an animal, a being or another identity in the life of the identity. The person it works with is a character too.
_Avoid_: person, user, contact

**Location**:
A place in the life of the identity. Its rooms and angles are location details beside it.
_Avoid_: place, room, scene

**Event**:
A happening worth recalling, with its story date. A defining event is one that changed the identity.
_Avoid_: memory, moment, scene

**Era**:
A named span of story time that groups events.
_Avoid_: period, chapter, phase

**Thread**:
A promise, a plan or a running bit that stays open until someone closes it.
_Avoid_: todo, task, session, conversation

**Conflict**:
Two sources that disagree about a fact. It stays open until one account wins.
_Avoid_: dispute, contradiction, merge conflict

**Lesson**:
An interpretation of experience, with the evidence for it.
_Avoid_: insight, takeaway, learning

**Log**:
The bullet list of the small things of one story day. Each day has one log.
_Avoid_: diary, daily note, journal

**Journal**:
A dated entry of private feelings about a day. Only journal entries link to it.
_Avoid_: diary, log

**Project**:
A piece of work that the identity takes part in. Its entry and its notes are in work.
_Avoid_: repo, workspace, job

**Note**:
A dated entry of the identity's own experience of work on one project.
_Avoid_: memo, log, soul note

**Profile**:
The entry in work that holds the roles, the working preferences and the past work of the identity.
_Avoid_: work readme, resume, bio

**KB**:
The knowledge base of a project, kept outside the identity. Facts about the project itself live there.
_Avoid_: docs, wiki, project memory

### State and review

**Stamp**:
The time the text of an entry last changed, recorded with the hash of that text. It moves only when the hash changes.
_Avoid_: timestamp, state timestamp, mtime

**State file**:
The file of one scope that holds the stamps of its entries.
_Avoid_: state, status, cache

**Story time**:
The calendar inside the life of the identity. The date of an entry is a story date, not the day of the chat.
_Avoid_: real time, chat time, timestamp

**Context**:
What a host loads at the start of a session so that the agent can be the identity: the load set, then the Now block.
_Avoid_: prompt, system prompt, memory

**Load set**:
The entries that the context holds, each in a tier.
_Avoid_: bundle, file list

**Tier**:
A group of the load set with one reason to load, such as core, voice or project.
_Avoid_: level, layer

**Now block**:
The generated summary of where the identity stands: last activity, recent days and events, open threads and conflicts, and the review status.
_Avoid_: state, status, current state

**KB pointer**:
The line in the context that tells the agent where the KB of the working project is.
_Avoid_: KB link, KB path

**Review**:
The pass that reads what changed since the last review, writes lessons, compiles MEMORY again and changes the soul when the evidence is enough.
_Avoid_: reflection, consolidation, compaction

**Review packet**:
What a review reads: MEMORY, the entries that changed, the unreviewed log days, and the open threads and conflicts.
_Avoid_: report, digest

**Soul change**:
One recorded change to a soul file, with its evidence.
_Avoid_: history record, soul note, edit

**Soul history**:
The list of the soul changes of an identity, oldest first.
_Avoid_: trail, changelog, audit log

### Sessions and sync

**Host**:
A program that runs an agent in sessions, such as Claude Code or a hosted chat.
_Avoid_: harness, client, platform

**Session**:
One chat between a user and an agent on a host, named by the session id of the host.
_Avoid_: conversation, chat, thread

**Transcript**:
The record that a host keeps of a session, as a file on this machine.
_Avoid_: history, chat log

**Turn**:
One text that the user or the assistant wrote in a session, with its role and its turn id.
_Avoid_: message, exchange

**User turn**:
A turn that the user wrote. Sync counts user turns.
_Avoid_: prompt, user message

**Sync**:
The work that records a session into an identity: it finds the new turns, writes a slice and gives it to an ingest.
_Avoid_: save, capture, import

**Sync run**:
One ingest of one slice of a session. It starts when the session is due, and it ends when the ingest agent reports it done. The short form is "run".
_Avoid_: job, batch, ingest run

**Run size**:
The count of new user turns at which a session is due.
_Avoid_: interval, threshold, batch size

**Due**:
A session is due when its new user turns reach the run size. The next sync starts a run.
_Avoid_: ready, pending

**Pending run**:
A run that has started and is not done.
_Avoid_: due run, open run

**Bookmark**:
The record of how far the ingest of a session has reached, with the pending run when there is one.
_Avoid_: cursor, offset, checkpoint

**Slice**:
The turns of one sync run, with two earlier user turns as overlap, written as a source.
_Avoid_: session slice, sync slice, chunk

**Push**:
One hand-over of turns from a host to sync, for a session with no transcript on this machine.
_Avoid_: store, job, upload

**Turns file**:
The file that holds the pushed turns of one session, in push order.
_Avoid_: store, store file

**Ingest**:
The agent task that turns a source into entries that cite it.
_Avoid_: import, extraction, memory write

**Ingest prompt**:
The text that gives an ingest agent the whole task of one sync run.
_Avoid_: prompt file, task file

**Ingest agent**:
The agent that runs one ingest prompt.
_Avoid_: background agent, worker

**Ingest command**:
The command line that starts an ingest agent.
_Avoid_: harness command, worker command

**Checkpoint**:
The three steps that an agent runs after it writes entries: stamp, index and lint.
_Avoid_: bookmark, save point

### Commands and hosts

**Operation**:
One thing that owo does, such as add or review done. Each operation is a CLI command and an MCP tool.
_Avoid_: action, endpoint

**Envelope**:
The JSON result of an operation: ok, data, warnings and errors.
_Avoid_: response, payload

**Dry run**:
A run of an operation that writes nothing and reports the files it would write.
_Avoid_: preview, simulation

**Selected identity**:
The identity that an operation acts on. It comes from the flag, the environment, the config, or the only identity in the root.
_Avoid_: current identity, active identity, picked identity

**Local config**:
The `.owo.toml` file that owo finds from the working folder up. It can set the root, the identity and the project.
_Avoid_: project file, project config

**User config**:
The config file in the user's config folder. It can set the root and the identity, but not the project.
_Avoid_: global config

**Working project**:
The project that context, state, sync and a new note use. It comes from the flag, the environment or the config.
_Avoid_: current project, active project

**Project filter**:
A project that narrows the result of a search or a card export. It comes only from the command line.
_Avoid_: project scope, working project

**Identity block**:
The text that the ACP proxy puts first in a prompt to give the agent the identity of the session.
_Avoid_: intro, text block, injection

**Export**:
A copy of an identity in another format: a character card, a SoulSpec package or a skill wrapper.
_Avoid_: dump, backup

**Character card**:
An export of an identity in the Character Card V2 or V3 format.
_Avoid_: card file, persona card

**SoulSpec**:
The outside format for a soul package. The soul manifest follows it, and an identity exports to it.
_Avoid_: soul format, spec
