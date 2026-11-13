//! Why a library call failed: the kind of failure, its text, and for the failures a caller may
//! word in its own terms, a typed `Fault`.

use crate::identity::Kind;
use std::path::PathBuf;

/// What kind of failure an `Error` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A bad argument, such as `add` with no title.
    Usage,
    /// A root, identity or project that does not resolve or does not exist.
    Config,
    /// A file that cannot be read or written.
    Io,
    /// Anything else the call could not do.
    Failed,
}

/// A failure with its facts, so a caller can say what to do in its own terms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// `add` with no title, for an entry type that needs one.
    NeedsTitle(Kind),
    /// A template name that no template has.
    UnknownTemplate(String),
    /// The selected identity has no folder at this path.
    NoIdentityFolder(PathBuf),
    /// The project has no `work/projects/<slug>/project.md`.
    NoProject(String),
    /// A write to a generated file; `Identity::index` writes it.
    Generated(String),
    /// `soul/state.json` is missing; `Identity::stamp` writes it.
    NoSoulState,
    /// Another sync step holds the lock file of the session.
    SessionBusy { session: String, lock: PathBuf },
    /// A session of pushed turns, given to `SessionSync::start`.
    PushedSession(String),
    /// A session that reads a host transcript, given to `SessionSync::push`.
    TranscriptSession { session: String, transcript: String },
    /// `Root::sync_done` for a session that no sync step started.
    NoBookmark(String),
    /// `Root::sync_done` past the messages the last sync step read.
    PastMessages { until: usize, seen: u64 },
    /// A number outside the range of its field, such as `days` outside 0 to `MAX_DAYS`.
    OutOfRange {
        name: &'static str,
        value: i64,
        min: i64,
        max: i64,
    },
    /// Two fields that cannot be given together, such as `since` and `days` of `changes`.
    Conflict(&'static str, &'static str),
    /// A field given without the field it needs, such as `png` without `out` of a card.
    Needs(&'static str, &'static str),
    /// The proxy option `inject` in a root with no identity.
    InjectNeedsIdentity,
}

impl Fault {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Fault::NeedsTitle(_)
            | Fault::OutOfRange { .. }
            | Fault::Conflict(..)
            | Fault::Needs(..)
            | Fault::InjectNeedsIdentity => ErrorKind::Usage,
            Fault::NoIdentityFolder(_) | Fault::NoProject(_) => ErrorKind::Config,
            _ => ErrorKind::Failed,
        }
    }
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fault::NeedsTitle(kind) => write!(f, "a {} entry needs a title", kind.name()),
            Fault::UnknownTemplate(name) => write!(f, "unknown template {name:?}"),
            Fault::NoIdentityFolder(dir) => {
                write!(f, "identity folder not found: {}", dir.display())
            }
            Fault::NoProject(p) => write!(f, "project not found: work/projects/{p}/project.md"),
            Fault::Generated(path) => write!(f, "{path} is generated"),
            Fault::NoSoulState => f.write_str("soul/state.json missing"),
            Fault::SessionBusy { session, lock } => write!(
                f,
                "session {session:?} is busy: another sync step holds {}",
                lock.display()
            ),
            Fault::PushedSession(s) => write!(f, "session {s:?} stores pushed turns"),
            Fault::TranscriptSession {
                session,
                transcript,
            } => write!(f, "session {session:?} syncs the transcript {transcript}"),
            Fault::NoBookmark(s) => write!(f, "no sync bookmark for session {s:?}"),
            Fault::PastMessages { until, seen } => write!(
                f,
                "until {until} is past the {seen} messages the last sync step read"
            ),
            Fault::OutOfRange {
                name,
                value,
                min,
                max,
            } => write!(f, "{name} must be from {min} to {max}, not {value}"),
            Fault::Conflict(a, b) => write!(f, "{a} and {b} cannot be used together"),
            Fault::Needs(a, b) => write!(f, "{a} needs {b}"),
            Fault::InjectNeedsIdentity => f.write_str("inject needs an identity in the root"),
        }
    }
}

impl std::error::Error for Fault {}

/// Why a call failed. `Display` is the whole error text.
#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    /// The failure with its facts, when it is one a caller may word itself.
    pub fault: Option<Fault>,
    message: String,
}

impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            fault: None,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<Fault> for Error {
    fn from(fault: Fault) -> Self {
        Self {
            kind: fault.kind(),
            message: fault.to_string(),
            fault: Some(fault),
        }
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        let message = format!("{e:#}");
        if let Some(fault) = e.downcast_ref::<Fault>() {
            return Self {
                kind: fault.kind(),
                fault: Some(fault.clone()),
                message,
            };
        }
        let kind = if e.downcast_ref::<crate::config::ConfigError>().is_some() {
            ErrorKind::Config
        } else if e.downcast_ref::<std::io::Error>().is_some() {
            ErrorKind::Io
        } else {
            ErrorKind::Failed
        };
        Self::new(kind, message)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
