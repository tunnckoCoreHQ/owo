# Small dependency set: small in-crate code instead of a crate

When a job needs only a small part of what a crate offers, owo writes that part in the crate.

- Search is about 100 lines of BM25 in `crates/core/src/search.rs`, rebuilt on each call, with no index cache. An identity has a few hundred files, so a scan takes milliseconds. Tantivy fits if identities grow past a few thousand files.
- Embedding a character card only appends `tEXt` chunks before `IEND`, so `crates/core/src/export.rs` walks PNG chunks with a small CRC32 and base64 instead of an image crate.
- The MCP server in `crates/core/src/mcp.rs` is a newline-delimited JSON-RPC loop with no MCP crate, and its `--remote` mode is a small HTTP/1.1 loop over `std::net`.
- Templates are compiled in with `include_str!` in `crates/core/src/templates.rs`, so the binary works with no install step. A file in the templates dir overrides one.
