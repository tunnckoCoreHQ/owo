use anyhow::{Context, Result};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, Permissions};
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};

thread_local! {
    static KEPT: RefCell<Option<Vec<Target>>> = const { RefCell::new(None) };
}

pub fn dry_run<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
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
    for path in kept.iter().flat_map(|t| {
        let dir = match t.bytes {
            Some(_) => t.path.parent(),
            None => Some(t.path.as_path()),
        };
        let folders = dir.map(missing).unwrap_or_default().into_iter().rev();
        folders
            .map(|d| format!("{}/", d.display()))
            .chain(t.bytes.is_some().then(|| t.path.display().to_string()))
    }) {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    (out, paths)
}

pub fn dry() -> bool {
    KEPT.with_borrow(Option::is_some)
}

fn kept(path: &Path) -> Option<Vec<u8>> {
    KEPT.with_borrow(|kept| {
        kept.iter()
            .flatten()
            .rev()
            .find(|t| t.bytes.is_some() && t.path == path)
            .and_then(|t| t.bytes.clone())
    })
}

pub fn kept_under(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    KEPT.with_borrow(|kept| {
        kept.iter()
            .flatten()
            .filter_map(|t| Some((t.path.strip_prefix(dir).ok()?, t.bytes.as_ref()?)))
            .map(|(rel, bytes)| (rel.to_path_buf(), bytes.clone()))
            .collect()
    })
}

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    kept(path).map_or_else(|| fs::read(path), Ok)
}

pub fn read_to_string(path: &Path) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
}

pub fn exists(path: &Path) -> bool {
    kept(path).is_some() || path.exists()
}

struct Target {
    path: PathBuf,
    shown: PathBuf,
    bytes: Option<Vec<u8>>,
}

#[derive(Default)]
#[must_use]
pub struct Writes(Vec<Target>);

impl Writes {
    fn push(mut self, path: PathBuf, shown: PathBuf, bytes: Option<Vec<u8>>) -> Self {
        self.0.push(Target { path, shown, bytes });
        self
    }

    pub fn file(
        self,
        path: impl Into<PathBuf>,
        shown: impl Into<PathBuf>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.push(path.into(), shown.into(), Some(bytes.into()))
    }

    pub fn json(
        self,
        path: impl Into<PathBuf>,
        shown: impl Into<PathBuf>,
        doc: &Value,
    ) -> Result<Self> {
        let text = format!("{}\n", serde_json::to_string_pretty(doc)?);
        Ok(self.file(path, shown, text))
    }

    pub fn dir(self, path: impl Into<PathBuf>, shown: impl Into<PathBuf>) -> Self {
        self.push(path.into(), shown.into(), None)
    }

    pub fn and(mut self, other: Writes) -> Self {
        self.0.extend(other.0);
        self
    }

    pub fn commit(self) -> Result<()> {
        if dry() {
            KEPT.with_borrow_mut(|kept| kept.as_mut().expect("a dry run").extend(self.0));
            return Ok(());
        }
        let mut undo = Undo::default();
        let result = apply(self.0, &mut undo);
        if result.is_err() {
            undo.run();
        }
        result
    }
}

struct Old {
    bytes: Vec<u8>,
    perms: Permissions,
}

#[derive(Default)]
struct Undo {
    made: Vec<PathBuf>,
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

enum Land {
    Rename { tmp: PathBuf, old: Option<Old> },
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

fn make_dirs(dir: &Path, shown: &Path, made: &mut Vec<PathBuf>) -> Result<()> {
    made.extend(missing(dir).into_iter().rev());
    fs::create_dir_all(dir).with_context(|| format!("creating {}", shown.display()))
}

pub fn missing(dir: &Path) -> Vec<PathBuf> {
    dir.ancestors()
        .take_while(|d| !d.as_os_str().is_empty() && fs::symlink_metadata(d).is_err())
        .map(Path::to_path_buf)
        .collect()
}

fn tmp(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

fn resolve(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Err(e) if e.kind() == ErrorKind::NotFound && fs::symlink_metadata(path).is_err() => {
            Ok(path.to_path_buf())
        }
        result => result,
    }
}

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

fn restore(path: &Path, old: &Old) -> io::Result<()> {
    let tmp = tmp(path);
    write_new(&tmp, &old.bytes, Some(&old.perms))?;
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

pub fn file(path: &Path, bytes: impl Into<Vec<u8>>) -> Result<()> {
    Writes::default().file(path, path, bytes).commit()
}

pub fn json(path: &Path, doc: &Value) -> Result<()> {
    Writes::default().json(path, path, doc)?.commit()
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
