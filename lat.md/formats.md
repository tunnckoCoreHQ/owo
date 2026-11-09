# Formats

The on-disk formats the code reads and writes. They follow `docs/design.md` sections 3 and 4.

## Identity layout

An identity is `<root>/<slug>/` with `identity.json`, `soul/`, `life/` and `work/`; `owo new` creates every folder and file listed here.

- `soul/`: `soul.json` (SoulSpec v0.5), `state.json`, `IDENTITY.md`, `SOUL.md`, `VOICE.md`, `LOOK.md`, `MEMORY.md`, `examples/{good,bad,greetings}.md`.
- `life/`: `state.json`, `_index.md`, `timeline.md`, and one folder per category: characters, locations, cultures, factions, events, eras, threads, conflicts, lessons, writing, log, journal.
- `work/`: `state.json`, `README.md`, `_index.md`, `projects/<p>/project.md`, `projects/<p>/notes/YYYY-MM-DD_<slug>.md`.

## Entry frontmatter

Every life and work entry has YAML frontmatter with `type`, `title`, `aliases`, `summary`, `status`, `created`, `updated`, `sources`. Lists are block lists (`  - item`); an empty list is `[]`.

Citations live only in frontmatter: `sources` (paths relative to `sources/`, optional `#L<a>-L<b>`, or plain text), lesson `evidence` and thread `from` are links. Bodies have no Sources, Evidence or From section.

`type` must match the folder. `status` is `canon`, `disputed` or `superseded`. Dates are `YYYY-MM-DD`. Dated types (log, journal, writing, note) carry `date`, and the file name must start with it. Events need `date`; threads need `state: open|closed`; conflicts need `state: open|resolved`; location details need `parent`; notes need `project` equal to their project folder.

## Soul files

Soul Markdown files are plain Markdown with no frontmatter; their stamps live in `soul/state.json`.

## State files

Each `state.json` has `files`: a map from scope-relative path to `{created, updated, hash}`. Hashes are `b3:<blake3 hex>` of the file text.

`soul/state.json` adds `history` (soul change trail) and `review` (`last_reviewed`, `reviewed_through`, `memory_hash`). `life/state.json` adds `last_write`. `work/state.json` adds `projects`.

## Generated files

Generated files start with a marker comment and depend only on entry content, so regeneration is deterministic and comparable.

Each life category `_index.md` lists `- [[Title]] (marks): summary (aliases: ...)`, where the marks name a `state` other than open (closed, resolved) and a `status` other than canon; location details nest under their overview. `life/_index.md` lists categories with counts. `life/timeline.md` groups events under eras sorted by `date`; `era` resolves like any link (title, alias, slug, `|label`). `work/_index.md` lists the profile and projects with note counts.

## Config resolution

Root resolves from `--root`, `OWO_ROOT`, `root` in the nearest `.owo.toml`, `root` in `$XDG_CONFIG_HOME/owo/config.toml`, then `$XDG_DATA_HOME/owo`.

Identity resolves from `-i`, `OWO_IDENTITY`, `.owo.toml`, user config, then the only identity in the root. Project resolves from `--project`, `OWO_PROJECT`, `.owo.toml`; see [[decisions#Working project versus project filter]]. Relative `root` values resolve against the config file's folder; `~/` expands to `$HOME`.

## Templates

Templates are embedded in the binary. `<root>/.templates/<name>` overrides one, where `<name>` is the identity-relative path for identity files (`soul/IDENTITY.md`, `work/README.md`) and `<type>.md` for entries (`character.md`).

Placeholders are `{{name}}`, `{{slug}}`, `{{title}}`, `{{today}}`, `{{date}}`, `{{parent}}`, `{{project}}`. A `{{key_yaml}}` form escapes double quotes for YAML strings.
