# owo

owo keeps a living agent identity as a folder of plain files. Hosts load the identity, record their sessions into it and reflect on it, so one identity lasts across hosts and projects.

## Identity

**Root**:
The folder that holds every **Identity**, the **Raw Material**, the **Global Config** and the session data. It comes from the flag, the environment or the **Local Config**, and otherwise it is the default data folder.

_Avoid: identities root, home, workspace_

**Identity**:
Who an agent can be, kept as one whole folder in the **Root**, such as `thea/`. It has three **Scopes**: **Persona**, **Life** and **Work**.

_Avoid: agent, persona, character, profile_

**Slug**:
The lowercase, hyphenated form of a name. It names an **Identity**, a **Project** and an **Entry** on disk.

_Avoid: id, key, handle_

**Scope**:
One of the three parts of an **Identity**: **Persona**, **Life** or **Work**.

_Avoid: part, side, section_

**Persona**:
The **Scope** that says who the **Identity** is, kept in the `persona/` folder. It changes slowly, and only through a **Reflection**, with **Evidence**.

_Avoid: soul, identity, personality, core_

**Life**:
The **Scope** that holds what the **Identity** lived and knows, written in its own voice.

_Avoid: world, lore, memory_

**Work**:
The **Scope** that holds the professional side of the **Identity**: its **Work Profile** and its own experience of each **Project**.

_Avoid: job, career_

**Persona File**:
One of the five **Entries** of the **Persona**: IDENTITY, SOUL, VOICE, LOOK and **MEMORY**.

_Avoid: soul file, persona entry, core file_

**Soul**:
The **Persona File** that holds the beliefs, the wants and the boundaries of the **Identity**, in the first person. **Soul** means only this file, `SOUL.md`, and never the whole **Persona**.

_Avoid: persona, personality, soul file_

**MEMORY**:
The **Persona File** that holds the compiled long-term memory of the **Identity**. Only a **Reflection** writes it.

_Avoid: memories, memory file_

## Entries

**Entry**:
One Markdown file that a person or an agent writes in an **Identity**. A **Persona File**, a **Character** and a **Project Note** are all **Entries**.

_Avoid: document, page, record, item_

**Kind**:
What an **Entry** is, such as a **Character**, an **Event** or an **Insight**. The path of the **Entry** decides it.

_Avoid: type, category, class_

**Type**:
The **Kind** that a **Life** or **Work** **Entry** declares in its frontmatter. It must equal the **Kind** that the path gives.

_Avoid: kind, category_

**Category**:
A folder of **Life** that holds the **Entries** of one **Kind**, such as characters or events.

_Avoid: collection, section, type_

**Status**:
How far an **Entry** stands as fact: canon, disputed or superseded.

_Avoid: state_

**Summary**:
The one-line description in the frontmatter of an **Entry**. Every **Index**, search and the **Current State** show it.

_Avoid: description, abstract_

**Wikilink**:
A `[[...]]` link from one **Entry** to another, by title, alias or **Slug**.

_Avoid: link, reference, pointer, wiki-link_

**Raw Material**:
The files outside every **Identity** that **Entries** cite, such as a **Transcript**, a note or a **Slice**.

_Avoid: source, reference, evidence_

**Evidence**:
The **Wikilinks** that support an **Insight** or a **Persona Change**.

_Avoid: raw material, proof, citation_

**Identifier**:
An exact personal value of a **Character**, such as a birth date, a handle or an address. It is written only in the **Entry** of that **Character**.

_Avoid: PII, id, personal data_

**Generated File**:
A file that owo builds from **Entry** content and that no person or agent edits: an **Index** or the **Timeline**.

_Avoid: derived file, cache, output_

**Index**:
The generated list of the **Entries** of one **Category** or one **Scope**, with their **Summaries**.

_Avoid: table of contents, catalog_

**Timeline**:
The generated list of **Events** in **Story Time**, grouped under **Eras**.

_Avoid: history, chronology_

## Kinds of entry

**Character**:
A person, an animal, a being or another **Identity** in the **Life** of the **Identity**. The person it works with is a **Character** too.

_Avoid: person, user, contact_

**Location**:
A place in the **Life** of the **Identity**. Its rooms and angles are location details beside it.

_Avoid: place, room, scene_

**Event**:
A happening worth recalling, with its story date. A defining **Event** is one that changed the **Identity**.

_Avoid: memory, moment, scene_

**Era**:
A named span of **Story Time** that groups **Events**.

_Avoid: period, chapter, phase_

**Thread**:
A promise, a plan or a running bit that stays open until someone closes it.

_Avoid: todo, task, session, conversation_

**Conflict**:
Two pieces of **Raw Material** that disagree about a fact. It stays open until one account wins.

_Avoid: dispute, contradiction, merge conflict_

**Insight**:
A life lesson that the **Identity** learned from what it lived, with the **Evidence** for it, kept in `life/insights/`. An **Identity** writes one only when its experience really taught it something.

_Avoid: lesson, takeaway, tip, learning_

**Daily Log**:
The bullet list of the small things of one story day. Each day has one **Daily Log**, with a file name that starts with its date, then a slug of its title.

_Avoid: log, diary, daily note, journal_

**Journal**:
A private diary **Entry** that the **Identity** writes whenever it wants, with a file name that starts with its date. Only another **Journal** links to it.

_Avoid: daily log, log_

**Project**:
A piece of work that the **Identity** takes part in. Its **Project Readme** and its **Project Notes** are in **Work**.

_Avoid: repo, workspace, job_

**Project Readme**:
The **Entry** of one **Project**, `work/projects/<project>/README.md`. It holds a description of the **Project** in one or two sentences, the role of the **Identity** on it and the **Project Wiki Path**.

_Avoid: project file, project entry, project profile_

**Project Note**:
A dated **Entry** of the **Identity**'s own experience on one **Project**.

_Avoid: note, memo, daily log_

**Work Profile**:
The **Entry** `work/PROFILE.md`, which holds the roles, the working preferences and the past work of the **Identity**.

_Avoid: profile, work readme, resume, bio_

**Project Wiki**:
The knowledge base of a **Project**, kept outside the **Identity**. Facts about the **Project** itself live there.

_Avoid: KB, docs, project memory_

**Project Wiki Path**:
The line that tells the agent where the **Project Wiki** of a **Project** is. The **Project Readme** holds it, and the **Context** repeats it for the **Working Project**.

_Avoid: KB pointer, wiki link, wiki pointer_

## State and reflection

**Stamp**:
The time the text of an **Entry** last changed, recorded with the hash of that text. It moves only when the hash changes.

_Avoid: timestamp, state timestamp, mtime_

**State File**:
The file of one **Scope** that holds the **Stamps** of its **Entries**, such as `persona/state.json`, `life/state.json` and `work/state.json`.

_Avoid: state, status, cache_

**Story Time**:
The calendar inside the **Life** of the **Identity**. The date of an **Entry** is a story date, not the day of the chat.

_Avoid: real time, chat time, timestamp_

**Context**:
What a **Host** loads at the start of a **Session** so that the agent can be the **Identity**: the **Load Set**, then the **Current State**.

_Avoid: prompt, system prompt, memory_

**Load Set**:
The **Entries** that the **Context** holds, each in a **Tier**.

_Avoid: bundle, file list_

**Tier**:
A group of the **Load Set** with one reason to load. The **Tiers** are core, voice, look and work.

_Avoid: level, layer_

**Current State**:
The generated overview of where the **Identity** stands: last activity, recent days and **Events**, open **Threads** and **Conflicts**, and when the last **Reflection** was.

_Avoid: now block, state, status_

**Reflection**:
The pass that reads what changed since the last **Reflection**, writes **Insights**, compiles **MEMORY** again and changes the **Persona** when the **Evidence** is enough. An agent runs it with `owo reflect` and finishes it with `owo reflect done`.

_Avoid: review, consolidation, compaction_

**Reflection Digest**:
What a **Reflection** reads: **MEMORY**, the **Entries** that changed, the **Daily Logs** since the last **Reflection**, and the open **Threads** and **Conflicts**.

_Avoid: review packet, packet, report_

**Persona Change**:
One recorded change to a **Persona File**, with its **Evidence**.

_Avoid: soul change, soul note, history record, edit_

**Persona Changelog**:
The list of the **Persona Changes** of an **Identity**, oldest first.

_Avoid: soul history, trail, audit log_

## Sessions and sync

**Host**:
A program that runs an agent in **Sessions**, such as Claude Code or a hosted chat.

_Avoid: harness, client, platform_

**Session**:
One chat between a user and an agent on a **Host**, named by the session id of the **Host**.

_Avoid: conversation, chat, thread_

**Transcript**:
The record that a **Host** keeps of a **Session**, as a file on the machine.

_Avoid: history, chat log_

**Turn**:
One text that the user or the assistant wrote in a **Session**, with its role and its turn id.

_Avoid: message, exchange_

**User Turn**:
A **Turn** that the user wrote. **Sync** counts **User Turns**.

_Avoid: prompt, user message_

**Sync**:
The work that records a **Session** into an **Identity**. It finds the new **Turns**, writes a **Slice** and gives it to an **Ingest**.

_Avoid: save, capture, import_

**Sync Run**:
One **Ingest** of one **Slice** of a **Session**. It starts when the **Session** is **Due**, and it ends when the **Ingest Agent** reports it done. The short form is "run".

_Avoid: job, batch, ingest run_

**Run Size**:
The count of new **User Turns** at which a **Session** is **Due**, 6 by default.

_Avoid: interval, threshold, batch size_

**Due**:
A **Session** is **Due** when its new **User Turns** reach the **Run Size**. The next **Sync** starts a **Sync Run**.

_Avoid: ready, pending_

**Pending Run**:
A **Sync Run** that has started and is not done.

_Avoid: due run, open run_

**Sync Cursor**:
The record of how far the **Ingest** of a **Session** has reached, with the **Pending Run** when there is one.

_Avoid: bookmark, offset, checkpoint_

**Slice**:
The **Turns** of one **Sync Run**, with two earlier **User Turns** as overlap, written as **Raw Material**.

_Avoid: session slice, sync slice, chunk_

**Push**:
One hand-over of **Turns** from a **Host** to **Sync**, for a **Session** with no **Transcript** on the machine.

_Avoid: store, job, upload_

**Turns File**:
The file that holds the pushed **Turns** of one **Session**, in **Push** order.

_Avoid: store, store file_

**Ingest**:
The agent task that turns **Raw Material** into **Entries** that cite it.

_Avoid: import, extraction, memory write_

**Ingest Prompt**:
The text that gives an **Ingest Agent** the whole task of one **Sync Run**.

_Avoid: prompt file, task file_

**Ingest Agent**:
The agent that runs one **Ingest Prompt**.

_Avoid: background agent, worker_

**Ingest Command**:
The command line that starts an **Ingest Agent**.

_Avoid: harness command, worker command_

**Checkpoint**:
The three steps that an agent runs after it writes **Entries**: it refreshes the **Stamps**, builds the **Generated Files** again and runs the lint.

_Avoid: sync cursor, save point_

## Commands and hosts

**Operation**:
One thing that owo does, such as add or reflect done. Each **Operation** is a CLI command and an MCP tool.

_Avoid: action, endpoint_

**Envelope**:
The JSON result of an **Operation**: ok, data, warnings and errors.

_Avoid: response, payload_

**Dry Run**:
A run of an **Operation** that writes nothing and reports the files it would write.

_Avoid: preview, simulation_

**Selected Identity**:
The **Identity** that an **Operation** acts on. It comes from the flag, the environment, the **Local Config**, the **Global Config**, or the only **Identity** in the **Root**.

_Avoid: current identity, active identity, picked identity, selected persona_

**Local Config**:
The `.owo.toml` file that owo finds from the working folder up. It can set the **Root**, the **Selected Identity** and the **Working Project**.

_Avoid: project file, project config_

**Global Config**:
The config file in the **Root**. It can set the **Selected Identity** and other values, but not the **Root**.

_Avoid: user config, root config_

**Working Project**:
The **Project** that the **Context**, the **Current State**, **Sync** and a new **Project Note** use. It comes from the flag, the environment or the **Local Config**.

_Avoid: current project, active project_

**Project Filter**:
A **Project** that narrows the result of a search or of a **Persona Card**. It comes only from the command line.

_Avoid: project scope, working project_

**Identity Block**:
The text that the ACP proxy puts first in a prompt to give the agent the **Identity** of the **Session**.

_Avoid: intro, text block, injection_

**Export**:
A copy of an **Identity** in another format: a **Persona Card**, a **SoulSpec** package or a skill wrapper.

_Avoid: dump, backup_

**Persona Card**:
An **Export** of an **Identity** in the Character Card V2 or V3 format.

_Avoid: character card, card file_

**SoulSpec**:
The outside format for a packaged **Persona**. The manifest of the **Persona** follows it, and an **Identity** exports to it.

_Avoid: soul format, spec_
