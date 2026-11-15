//! All-or-nothing writes. Each file goes through a temporary file next to it and a rename, and
//! the files of one `Writes` land together or not at all, with the folders it made. In a dry
//! run (`dry_run`) a commit keeps its files in memory instead, and `read` sees them.

use anyhow::{Context, Result};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, Permissions};
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};

thread_local! {
    /// The targets the commits of the dry run on this thread kept, in commit order; None when
    /// no dry run runs.
    static KEPT: RefCell<Option<Vec<Target>>> = const { RefCell::new(None) };
}

/// Runs `f` with every commit kept in memory instead of written, and `read` seeing the kept
/// files. Returns what `f` returns and the paths the commits named, each once, in the order
/// they came; a folder ends in `/`.
pub fn dry_run<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    /// Puts back the state before the dry run, also when `f` panics.
    struct Reset(Option<Vec<Target>>);
    impl Drop for Reset {
        fn drop(&mut self) {
            KEPT.set(self.0.take());
        }
    }
    let reset = Reset(KEPT.replace(Some(vec![])));
    let out = f();
    let kept = KEPT.take().unwrap_or_default();
    drop(reset);
    let mut paths: Vec<String> = vec![];
    for t in kept {
        let mut path = t.path.display().to_string();
        if t.bytes.is_none() {
            path.push('/');
        }
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    (out, paths)
}

/// True while a dry run runs on this thread.
pub fn dry() -> bool {
    KEPT.with_borrow(Option::is_some)
}

/// The bytes the dry run last kept for the file `path`.
fn kept(path: &Path) -> Option<Vec<u8>> {
    KEPT.with_borrow(|kept| {
        kept.iter()
            .flatten()
            .rev()
            .find(|t| t.bytes.is_some() && t.path == path)
            .and_then(|t| t.bytes.clone())
    })
}

/// The files the dry run kept under `dir`, by their path relative to it, with their last bytes.
pub fn kept_under(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    KEPT.with_borrow(|kept| {
        kept.iter()
            .flatten()
            .filter_map(|t| Some((t.path.strip_prefix(dir).ok()?, t.bytes.as_ref()?)))
            .map(|(rel, bytes)| (rel.to_path_buf(), bytes.clone()))
            .collect()
    })
}

/// The file at `path`: in a dry run the bytes it kept, else the file on disk.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    kept(path).map_or_else(|| fs::read(path), Ok)
}

/// The file at `path` as UTF-8 text, as `read` gives it.
pub fn read_to_string(path: &Path) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
}

/// True when the file `path` is there: kept by the dry run or on disk.
pub fn exists(path: &Path) -> bool {
    kept(path).is_some() || path.exists()
}

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

    /// Writes every target, or on an error undoes what it did and returns the error. In a dry
    /// run it keeps the targets in memory and writes nothing.
    pub fn commit(self) -> Result<()> {
        let Some(targets) = KEPT.with_borrow_mut(|kept| match kept {
            Some(kept) => {
                kept.extend(self.0);
                None
            }
            None => Some(self.0),
        }) else {
            return Ok(());
        };
        let mut undo = Undo::default();
        let result = apply(targets, &mut undo);
        if result.is_err() {
            undo.run();
        }
        result
    }
}

/// A file before the commit replaced it.
struct Old {
    bytes: Vec<u8>,
    perms: Permissions,
}

/// What a commit did so far, to undo it.
#[derive(Default)]
struct Undo {
    made: Vec<PathBuf>,
    /// Temporary files this commit made, not yet renamed.
    temps: Vec<PathBuf>,
    replaced: Vec<(PathBuf, Option<Old>)>,
}

impl Undo {
    fn run(self) {
        for tmp in &self.temps {
            let _ = fs::remove_file(tmp);
        }
        for (path, old) in self.replaced.into_iter().rev() {
            let _ = match old {
                Some(old) => restore(&path, &old),
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
    Rename { tmp: PathBuf, old: Option<Old> },
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
        let writing = || format!("writing {}", t.shown.display());
        let path = resolve(&t.path).with_context(writing)?;
        let old = match fs::metadata(&path) {
            Err(e) if e.kind() == ErrorKind::NotFound => None,
            Err(e) => return Err(e).with_context(writing),
            Ok(meta) => {
                // A file the caller cannot write is not replaced.
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .with_context(writing)?;
                let bytes =
                    fs::read(&path).with_context(|| format!("reading {}", t.shown.display()))?;
                Some(Old {
                    bytes,
                    perms: meta.permissions(),
                })
            }
        };
        let tmp = tmp(&path);
        write_new(&tmp, &bytes, old.as_ref().map(|o| &o.perms)).with_context(writing)?;
        undo.temps.push(tmp.clone());
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

/// `path`, or the file its symlink points at, as an in-place write would write. A symlink whose
/// file is missing is an error, so the link is not replaced.
fn resolve(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Err(e) if e.kind() == ErrorKind::NotFound && fs::symlink_metadata(path).is_err() => {
            Ok(path.to_path_buf())
        }
        result => result,
    }
}

/// Makes the new file `path` holding `bytes`, synced, with `perms` when given. Fails when `path`
/// exists, also as a symlink, and on a later error removes only the file it made.
fn write_new(path: &Path, bytes: &[u8], perms: Option<&Permissions>) -> io::Result<()> {
    let mut file = File::options().write(true).create_new(true).open(path)?;
    fill(&mut file, bytes, perms).inspect_err(|_| {
        let _ = fs::remove_file(path);
    })
}

fn fill(file: &mut File, bytes: &[u8], perms: Option<&Permissions>) -> io::Result<()> {
    if let Some(perms) = perms {
        file.set_permissions(perms.clone())?;
    }
    file.write_all(bytes)?;
    file.sync_all()
}

/// Puts `old` back at `path` through a temporary file and a rename.
fn restore(path: &Path, old: &Old) -> io::Result<()> {
    let tmp = tmp(path);
    write_new(&tmp, &old.bytes, Some(&old.perms))?;
    fs::rename(&tmp, path).inspect_err(|_| {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tempfile::TempDir;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn chmod(path: &Path, mode: u32) {
        fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
    }

    fn error(path: &Path) -> String {
        format!("{:#}", file(path, "new").unwrap_err())
    }

    #[test]
    fn error_names_the_file_not_its_temporary_file() {
        let tmp = TempDir::new().unwrap();
        let ro = tmp.path().join("ro");
        fs::create_dir(&ro).unwrap();
        chmod(&ro, 0o555);
        let path = ro.join("x.json");
        let err = error(&path);
        chmod(&ro, 0o755);
        assert!(
            err.starts_with(&format!("writing {}: ", path.display())),
            "{err}"
        );
        assert!(!err.contains("x.json.tmp"), "{err}");
    }

    #[test]
    fn an_existing_temporary_file_is_kept() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("state.json");
        fs::write(tmp.path().join("state.json.tmp"), "keep").unwrap();
        error(&path);
        assert_eq!(
            fs::read_to_string(tmp.path().join("state.json.tmp")).unwrap(),
            "keep"
        );
        assert!(!path.exists());
    }

    #[test]
    fn a_symlink_at_the_temporary_name_is_kept() {
        let tmp = TempDir::new().unwrap();
        let other = tmp.path().join("other");
        fs::write(&other, "keep").unwrap();
        let link = tmp.path().join("x.tmp");
        symlink(&other, &link).unwrap();
        error(&tmp.path().join("x"));
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read_to_string(&other).unwrap(), "keep");
    }

    #[test]
    fn a_replace_keeps_the_mode() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("x");
        fs::write(&path, "old").unwrap();
        chmod(&path, 0o600);
        file(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn a_read_only_file_is_not_replaced() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("x");
        fs::write(&path, "old").unwrap();
        chmod(&path, 0o444);
        let err = error(&path);
        assert!(
            err.starts_with(&format!("writing {}: ", path.display())),
            "{err}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert!(!tmp.path().join("x.tmp").exists());
    }

    #[test]
    fn a_dangling_symlink_is_kept() {
        let tmp = TempDir::new().unwrap();
        let link = tmp.path().join("x");
        symlink(tmp.path().join("missing"), &link).unwrap();
        let err = error(&link);
        assert!(
            err.starts_with(&format!("writing {}: ", link.display())),
            "{err}"
        );
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert!(!tmp.path().join("missing").exists());
    }

    #[test]
    fn a_dry_run_ends_when_its_call_panics() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("x");
        let panicked = std::panic::catch_unwind(|| dry_run::<()>(|| panic!("boom"))).is_err();
        assert!(panicked && !dry());
        file(&path, "new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn an_unreadable_file_fails_as_a_read() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("x");
        fs::write(&path, "old").unwrap();
        chmod(&path, 0o200);
        let err = error(&path);
        chmod(&path, 0o600);
        assert!(
            err.starts_with(&format!("reading {}: ", path.display())),
            "{err}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
    }
}
