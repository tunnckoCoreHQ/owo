# Architecture

owo is one Rust binary crate. Each command loads an identity folder into memory, works on files only, and prints text or a JSON envelope.

## Modules

Each module owns one job; `main.rs` parses arguments with clap and dispatches.

- `config.rs` resolves root, identity, project and KB. See [[formats#Config resolution]].
- `identity.rs` walks one identity folder and parses every Markdown file into an `Entry` with a `Kind` decided by its path.
- `markdown.rs` splits frontmatter and parses it with `serde-saphyr` into `serde_json` values, reads sections, extracts wiki-links outside code, counts words.
- `slug.rs` holds the one slug function.
- `templates.rs` embeds every template with `include_str!` and fills `{{key}}` placeholders.
- `create.rs` implements `owo new`; `add.rs` implements `owo add`.
- `index.rs` generates `_index.md` files and `life/timeline.md`.
- `state.rs` and `stamp.rs` hash files and maintain the three `state.json` files.
- `lint.rs` runs the rules in [[lint]].
- `find.rs` resolves names, shows sections, and lists backlinks, outlinks and orphans.
- `status.rs` derives current state and the Now block; `context.rs` builds the tiered load set.
- `search.rs` is the BM25 ranker; `review.rs` builds the review packet, records finished reviews, and keeps the soul history.
- `git.rs` wraps the `git` binary; `changes.rs` implements `owo changes` and the changed-file set for `lint --changed`.
- `export.rs` writes character cards (JSON or PNG), SoulSpec packages and skill wrappers.
- `sync.rs` parses transcripts and writes session slices; `migrate.rs` converts v1 identities.
- `output.rs` defines diagnostics, the envelope and exit codes.
- `mcp.rs` serves the MCP tools over stdio; see [[decisions#MCP runs the CLI]].

## Loading an identity

`Identity::load` reads `identity.json`, then walks the folder in file-name order and parses each `.md` file once.

Path decides the kind: `life/events/x.md` is an event, `life/locations/<slug>/overview.md` a location, any other file in that folder a location detail, `_index.md` and `life/timeline.md` are generated. Files outside the layout get `Kind::Unknown`, which lint reports.

## Link resolution

Links resolve through one lowercase key map built from titles, aliases, file stems (folder name for `overview.md`), identity-relative paths and soul file names.

`resolve_link` tries the raw lowercase target, then its slug. Generated files are left out of the map, so links always land on authored entries.
