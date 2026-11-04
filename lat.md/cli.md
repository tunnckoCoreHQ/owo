# CLI

Commands implemented so far. Every command accepts `--root`, `-i/--identity`, `--project`, `--json` and `--quiet`.

## Output

With `--json` every command prints `{ "ok", "data", "warnings", "errors" }`; otherwise readable text.

Exit codes: 0 success, 1 problems found (lint errors, exists, unknown template, stale check), 2 usage error (clap), 3 config error (no identity selected), 4 I/O error.

## Commands

The implemented command set; see `docs/design.md` section 7 for the full plan.

- `owo new <name> [--slug]` creates an identity from templates, writes indexes and state files. Exits 1 when the folder exists.
- `owo add <type> <title> [--date] [--parent] [--alias]...` creates one entry at its deterministic path. Uses `--project` for notes. A second `add log` for the same day succeeds and prints `exists <path>`.
- `owo lint [--rule <code>]...` runs [[lint]] rules. Exits 1 when any error is found.
- `owo index [--check]` writes generated files, or reports stale ones and exits 1.
- `owo stamp [--check]` refreshes state files from hashes; see [[decisions#Stamp touches frontmatter]].
- `owo list`, `owo slug <text>`, `owo templates [name]`, `owo config`.
