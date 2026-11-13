# Every write is all or nothing

An identity is plain files that people, agents and owo all edit ([ADR-0001](0001-identity-is-plain-markdown-files.md)), so a write that stops halfway leaves a state that no command can name: a cut-off entry, an index that disagrees with its entries, or a sync slice with no bookmark. Only JSON files went through a temporary file and a rename, and a command that writes several files could stop after some of them.

Every file write in the core goes through `write::Writes`. Each file is written to `<name>.tmp` next to it, synced, and renamed over the file, so a killed write never leaves a cut-off file, and a failed write removes its temporary file. The files of one `Writes` are one set: all temporary files are written before the first rename, and when any step fails, the commit puts back the files it replaced, removes the files and the folders it made, and returns the error. A command that writes several files builds one `Writes` and commits it once, so it leaves all of its changes or none.

A write follows a symlink to its file, as an in-place write did. A device or a pipe, such as `--out /dev/stdout`, is written in place, since no rename can replace it. A rename needs write access to the folder, so a file in a read-only folder cannot be replaced.
