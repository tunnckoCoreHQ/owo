//! All-or-nothing writes. Each file goes through a temporary file next to it and a rename, and
//! the files of one `Writes` land together or not at all, with the folders it made.

use anyhow::{Context, Result};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

/// One target: its path, the path error text names, and its bytes, or none for a folder.
struct Target {
    path: PathBuf,
    shown: PathBuf,
    bytes: Option<Vec<u8>>,
}

/// Files and folders to write together. `commit` writes all of them or none.
#[derive(Default)]
#[must_use]
pub struct Writes(Vec<Target>);

impl Writes {
    pub fn new() -> Self {
        Self::default()
    }

    /// With the file `path` holding `bytes`; error text names it as `shown`.
    pub fn file(
        mut self,
        path: impl Into<PathBuf>,
        shown: impl Into<PathBuf>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.0.push(Target {
            path: path.into(),
            shown: shown.into(),
            bytes: Some(bytes.into()),
        });
        self
    }

    /// With the file `path` holding `doc` as pretty JSON and a trailing newline.
    pub fn json(
        self,
        path: impl Into<PathBuf>,
        shown: impl Into<PathBuf>,
        doc: &Value,
    ) -> Result<Self> {
        let text = format!("{}\n", serde_json::to_string_pretty(doc)?);
        Ok(self.file(path, shown, text))
    }

    /// With the folder `path`, made when it is missing.
    pub fn dir(mut self, path: impl Into<PathBuf>, shown: impl Into<PathBuf>) -> Self {
        self.0.push(Target {
            path: path.into(),
            shown: shown.into(),
            bytes: None,
        });
        self
    }

    /// With the targets of `other` after these.
    pub fn and(mut self, other: Writes) -> Self {
        self.0.extend(other.0);
        self
    }

    /// Writes every target, or on an error undoes what it did and returns the error.
    pub fn commit(self) -> Result<()> {
        let mut undo = Undo::default();
        let result = apply(self.0, &mut undo);
        if result.is_err() {
            undo.run();
        }
        result
    }
}

/// What a commit did so far, to undo it.
#[derive(Default)]
struct Undo {
    made: Vec<PathBuf>,
    temps: Vec<PathBuf>,
    replaced: Vec<(PathBuf, Option<Vec<u8>>)>,
}

impl Undo {
    fn run(self) {
        for tmp in &self.temps {
            let _ = fs::remove_file(tmp);
        }
        for (path, old) in self.replaced.into_iter().rev() {
            let _ = match old {
                Some(bytes) => restore(&path, &bytes),
                None => fs::remove_file(&path),
            };
        }
        for dir in self.made.iter().rev() {
            let _ = fs::remove_dir(dir);
        }
    }
}

/// How one file lands once every temporary file is written.
enum Land {
    /// Renamed over the file, which held `old` before.
    Rename { tmp: PathBuf, old: Option<Vec<u8>> },
    /// Written in place: a device or a pipe, such as `/dev/stdout`, which no rename can replace.
    InPlace(Vec<u8>),
}

fn apply(targets: Vec<Target>, undo: &mut Undo) -> Result<()> {
    let mut files = vec![];
    for t in targets {
        let Some(bytes) = t.bytes else {
            make_dirs(&t.path, &t.shown, &mut undo.made)?;
            continue;
        };
        if fs::metadata(&t.path).is_ok_and(|m| !m.is_file() && !m.is_dir()) {
            files.push((t.path, t.shown, Land::InPlace(bytes)));
            continue;
        }
        if let Some(dir) = t.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            make_dirs(dir, t.shown.parent().unwrap_or(&t.shown), &mut undo.made)?;
        }
        // Write through a symlink to the file it points at, as an in-place write would.
        let path = fs::canonicalize(&t.path).unwrap_or(t.path);
        let old = match fs::read(&path) {
            Ok(old) => Some(old),
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => return Err(e).with_context(|| format!("writing {}", t.shown.display())),
        };
        let tmp = tmp(&path);
        undo.temps.push(tmp.clone());
        write_synced(&tmp, &bytes)
            .with_context(|| format!("writing {}", self::tmp(&t.shown).display()))?;
        files.push((path, t.shown, Land::Rename { tmp, old }));
    }
    for (path, shown, land) in files {
        let writing = || format!("writing {}", shown.display());
        match land {
            Land::InPlace(bytes) => fs::write(&path, bytes).with_context(writing)?,
            Land::Rename { tmp, old } => {
                fs::rename(&tmp, &path).with_context(writing)?;
                undo.temps.retain(|t| *t != tmp);
                undo.replaced.push((path, old));
            }
        }
    }
    Ok(())
}

/// Makes `dir` and its missing parents; records each folder it made in `made`.
fn make_dirs(dir: &Path, shown: &Path, made: &mut Vec<PathBuf>) -> Result<()> {
    made.extend(missing(dir).into_iter().rev());
    fs::create_dir_all(dir).with_context(|| format!("creating {}", shown.display()))
}

/// The folders of `dir` and its parents that do not exist, deepest first.
pub fn missing(dir: &Path) -> Vec<PathBuf> {
    dir.ancestors()
        .take_while(|d| !d.as_os_str().is_empty() && fs::symlink_metadata(d).is_err())
        .map(Path::to_path_buf)
        .collect()
}

/// `path` with `.tmp` added to its file name.
fn tmp(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

fn write_synced(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Puts `bytes` back at `path` through a temporary file and a rename.
fn restore(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = tmp(path);
    write_synced(&tmp, bytes)
        .and_then(|()| fs::rename(&tmp, path))
        .inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
}

/// Writes one file all or nothing; error text names `path`.
pub fn file(path: &Path, bytes: impl Into<Vec<u8>>) -> Result<()> {
    Writes::new().file(path, path, bytes).commit()
}

/// Writes one JSON file all or nothing; error text names `path`.
pub fn json(path: &Path, doc: &Value) -> Result<()> {
    Writes::new().json(path, path, doc)?.commit()
}
