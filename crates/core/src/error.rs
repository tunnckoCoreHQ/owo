use crate::{Kind, Named};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Usage,
    Config,
    Io,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    NoRoot,
    NoIdentity(PathBuf),
    NeedsTitle(Kind),
    OnlyFor {
        field: &'static str,
        noun: &'static str,
        kind: Kind,
        given: Kind,
    },
    KindNeeds {
        kind: Kind,
        field: &'static str,
    },
    TemplateLacks {
        template: String,
        line: &'static str,
        field: &'static str,
    },
    NoText(String),
    Empty(&'static str),
    Field {
        name: &'static str,
        error: String,
    },
    OutNoSlug,
    SinceNotDay(String),
    UnknownTemplate(String),
    NoIdentityFolder(PathBuf),
    NoProject(String),
    Generated(String),
    NoPersonaState,
    SessionBusy {
        session: String,
        lock: PathBuf,
    },
    PushedSession(String),
    TranscriptSession {
        session: String,
        transcript: String,
    },
    NoMessages {
        transcript: PathBuf,
        format: &'static str,
    },
    NoBookmark(String),
    BehindBookmark {
        until: usize,
        bookmark: usize,
    },
    PastMessages {
        until: usize,
        seen: u64,
    },
    OutOfRange {
        name: &'static str,
        value: i64,
        min: i64,
        max: i64,
    },
    Conflict(&'static str, &'static str),
    Needs(&'static str, &'static str),
    InjectNeedsIdentity,
    NoIngestCommand(String),
}

impl Fault {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Fault::NeedsTitle(_)
            | Fault::OutOfRange { .. }
            | Fault::Conflict(..)
            | Fault::Needs(..)
            | Fault::InjectNeedsIdentity => ErrorKind::Usage,
            Fault::NoRoot
            | Fault::NoIdentity(_)
            | Fault::NoIdentityFolder(_)
            | Fault::NoProject(_)
            | Fault::NoIngestCommand(_) => ErrorKind::Config,
            _ => ErrorKind::Failed,
        }
    }
}

fn article(word: &str) -> &'static str {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fault::NoRoot => f.write_str("cannot resolve a root"),
            Fault::NoIdentity(root) => write!(f, "no identity selected in {}", root.display()),
            Fault::NeedsTitle(kind) => {
                let name = kind.name();
                write!(f, "{} {name} entry needs a title", article(name))
            }
            Fault::OnlyFor {
                field,
                noun,
                kind,
                given,
            } => write!(
                f,
                "{field} is for {}; a {} has no {noun}",
                kind.name(),
                given.name()
            ),
            Fault::KindNeeds { kind, field } => write!(f, "{} needs {field}", kind.name()),
            Fault::TemplateLacks {
                template,
                line,
                field,
            } => write!(
                f,
                "the {template} template has no `{line}` line for {field}"
            ),
            Fault::NoText(path) => write!(f, "no text for {path}"),
            Fault::Empty(field) => write!(f, "{field} is empty"),
            Fault::Field { name, error } => write!(f, "{name}: {error}"),
            Fault::OutNoSlug => f.write_str("out must end in the identity slug"),
            Fault::SinceNotDay(e) => write!(f, "outside Git, since takes a day: {e}"),
            Fault::UnknownTemplate(name) => write!(f, "unknown template {name:?}"),
            Fault::NoIdentityFolder(dir) => {
                write!(f, "identity folder not found: {}", dir.display())
            }
            Fault::NoProject(p) => write!(f, "project not found: work/projects/{p}/README.md"),
            Fault::Generated(path) => write!(f, "{path} is generated"),
            Fault::NoPersonaState => f.write_str("persona/state.json missing"),
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
            Fault::NoMessages { transcript, format } => write!(
                f,
                "no user or assistant messages in {} as {format}",
                transcript.display()
            ),
            Fault::NoBookmark(s) => write!(f, "no sync bookmark for session {s:?}"),
            Fault::BehindBookmark { until, bookmark } => {
                write!(f, "until {until} is behind the bookmark ({bookmark})")
            }
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
            Fault::NoIngestCommand(agent) => write!(
                f,
                "auto_ingest needs ingest_command: owo knows no ingest command for the agent {agent:?}"
            ),
        }
    }
}

impl std::error::Error for Fault {}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
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
        match e.downcast_ref::<Fault>() {
            Some(fault) => fault.clone().into(),
            None if e.is::<std::io::Error>() => Self::new(ErrorKind::Io, format!("{e:#}")),
            None => Self::new(ErrorKind::Failed, format!("{e:#}")),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
