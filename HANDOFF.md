# Handoff

Remaining work, in order. Everything else is done; `lat.md/` and `docs/design.md` describe the current behavior.

## Phase 1: remote MCP

The `owoid.calldata.space` tunnel makes `owo mcp --remote` reachable from the internet, and the HTTP mode has no authentication (see `lat.md/decisions.md`, "MCP runs the CLI"). Anyone with the URL can read the identity and run tools that write outside it: `export_* --out`, `migrate_v1 --out`.

1. Decide with the user: a bearer token (for example `OWO_MCP_TOKEN`, checked on every request), or keep the URL private and accept the risk.
2. A host that has only the MCP tools can create entries with `add` but cannot fill them. Decide whether a write tool is needed (an identity-relative `.md` path plus content, no writes outside the identity folder). Do it together with item 1, because a write tool on an open URL lets anyone rewrite the identity.

## Phase 2: MCP tool table

`src/mcp.rs` declares every flag again next to the clap definitions in `src/main.rs`, so a new flag goes in two places. Building the table from `Cli::command()` removes about 180 lines. Tool descriptions and integer types then come from clap, so compare `tools/list` before and after.

## Phase 3: small items

- `owo soul note` records the same change with the same evidence again without a warning.
- `owo export card` gives no warning when `first_mes` and `mes_example` come out empty.
- An untitled `owo add log` writes a body of `- `, and that empty day counts as a recent and unreviewed day.
