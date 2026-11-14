# Identity is plain Markdown files, and the path decides the kind

An identity is a folder of plain Markdown files with frontmatter. There is no database: each command loads the folder into memory, works on the files, and writes files back.

The entry kind comes from its path, and frontmatter `type` must agree. One source of truth for placement keeps `owo add`, lint and the indexes consistent.

The three `state.json` files track a content hash for each file, so `owo stamp` and `stamp --check` see an edit without a database. Generated files (the `_index.md` files and `life/timeline.md`) depend only on entry content and carry no dates, so `owo index --check` and the `generated` lint rule can compare byte for byte.
