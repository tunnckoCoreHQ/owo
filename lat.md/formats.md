# Formats

The on-disk formats the code reads and writes. They follow `docs/design.md` sections 3 and 4.

## Identity layout

An identity is `<root>/<slug>/` with `identity.json`, `soul/`, `life/` and `work/`; `owo new` creates every folder and file listed here.

- `soul/`: `soul.json` (SoulSpec v0.5), `state.json`, `IDENTITY.md`, `SOUL.md`, `VOICE.md`, `LOOK.md`, `MEMORY.md`, `examples/{good,bad,greetings}.md`.
- `life/`: `state.json`, `_index.md`, `timeline.md`, and one folder per category: characters, locations, cultures, factions, events, eras, threads, conflicts, lessons, writing, log, journal.
- `work/`: `state.json`, `README.md`, `_index.md`, `projects/<p>/project.md`, `projects/<p>/notes/YYYY-MM-DD_<slug>.md`.

## Entry frontmatter

Every life and work entry has YAML frontmatter with `type`, `title`, `aliases`, `summary`, `status`, `created`, `updated`, `sources`.

`type` must match the folder. `status` is `canon`, `disputed` or `superseded`. Dates are `YYYY-MM-DD`. Dated types (log, journal, writing, note) carry `date`, and the file name must start with it. Events need `date`; threads need `state: open|closed`; conflicts need `state: open|resolved`; location details need `parent`; notes need `project` equal to their project folder.

## Soul files

Soul Markdown files are plain Markdown with no frontmatter; their stamps live in `soul/state.json`.

## State files

Each `state.json` has `files`: a map from scope-relative path to `{created, updated, hash}`. Hashes are `b3:<blake3 hex>` of the file text.

`soul/state.json` adds `history` (soul change trail) and `review` (`last_reviewed`, `reviewed_through`, `memory_hash`). `life/state.json` adds `last_write`. `work/state.json` adds `projects`.

## Generated files

Generated files start with a marker comment and depend only on entry content, so regeneration is deterministic and comparable.

Each life category `_index.md` lists `- [[Title]]: summary (aliases: ...)`; location details nest under their overview. `life/_index.md` lists categories with counts. `life/timeline.md` groups events under eras (by `era` link) sorted by `date`. `work/_index.md` lists the profile and projects with note counts.

## Config resolution

Root resolves from `--root`, `OWO_ROOT`, `root` in the nearest `.owo.toml`, `root` in `$XDG_CONFIG_HOME/owo/config.toml`, then `$XDG_DATA_HOME/owo`.

Identity resolves from `-i`, `OWO_IDENTITY`, `.owo.toml`, user config, then the only identity in the root. Project resolves from `--project`, `OWO_PROJECT`, `.owo.toml`. Relative `root` values resolve against the config file's folder; `~/` expands to `$HOME`.

## Templates

Templates are embedded in the binary; a file with the same name in `<root>/.templates/` overrides one.

Placeholders are `{{name}}`, `{{slug}}`, `{{title}}`, `{{today}}`, `{{date}}`, `{{parent}}`, `{{project}}`. A `{{key_yaml}}` form escapes double quotes for YAML strings.
