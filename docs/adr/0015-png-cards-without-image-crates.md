# PNG cards without image crates

Embedding a card only appends `tEXt` chunks before `IEND`. So `export.rs` walks PNG chunks with a small CRC32 and base64 instead of an image dependency.
