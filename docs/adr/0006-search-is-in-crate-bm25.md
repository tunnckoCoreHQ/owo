# Search is in-crate BM25

The `owo search` command is about 100 lines of BM25 in `search.rs`, rebuilt on each call, with no dependency and no cache. An identity has a few hundred files, so a scan takes milliseconds. Tantivy fits if identities grow past a few thousand files.
