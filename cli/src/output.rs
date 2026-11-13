//! What one command prints: the JSON envelope or the text, the warnings, the error and the exit
//! code.

use owo_core::{Envelope, ErrorKind, Fault};
use serde_json::Value;

/// Exit codes: 0 ok, 1 problems found, 2 usage, 3 config, 4 I/O.
pub mod exit {
    pub const OK: i32 = 0;
    pub const PROBLEMS: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const CONFIG: i32 = 3;
    pub const IO: i32 = 4;

    /// OK when the command found nothing wrong, else PROBLEMS.
    pub fn of(ok: bool) -> i32 {
        if ok {
            OK
        } else {
            PROBLEMS
        }
    }
}

/// What one command produced.
pub struct Output {
    /// The `{ ok, data, warnings, errors }` envelope, the result with `--json`.
    pub envelope: Value,
    /// The text result without `--json`, for stdout.
    pub text: String,
    /// Warnings for stderr: config warnings, plus the envelope's text warnings without `--json`.
    pub warnings: Vec<String>,
    /// The error for stderr without `--json`. A `clap::Error` is a usage error.
    pub error: Option<anyhow::Error>,
    pub code: i32,
}

impl Output {
    /// The warnings, one `owo: warning: <text>` line each.
    pub fn warning_lines(&self) -> String {
        self.warnings
            .iter()
            .map(|w| format!("owo: warning: {w}\n"))
            .collect()
    }

    /// The stdout text and the stderr text. Stderr holds the warnings, then a usage error as
    /// clap words it or the error line `owo: <error>`; stdout holds the JSON envelope when
    /// `json` is set, else the text.
    pub fn render(&self, json: bool) -> (String, String) {
        let mut stderr = self.warning_lines();
        if let Some(usage) = self.error.as_ref().and_then(|e| e.downcast_ref()) {
            return usage_text(usage, stderr);
        }
        let stdout = if json {
            let text = serde_json::to_string_pretty(&self.envelope).expect("envelope serializes");
            format!("{text}\n")
        } else {
            if let Some(e) = &self.error {
                stderr.push_str(&format!("owo: {e:#}\n"));
            }
            self.text.clone()
        };
        (stdout, stderr)
    }
}

/// The stdout and stderr text of a clap error, as clap prints it: help and version on stdout,
/// errors on stderr after `stderr`.
fn usage_text(usage: &clap::Error, mut stderr: String) -> (String, String) {
    let text = usage.render().to_string();
    if usage.use_stderr() {
        stderr.push_str(&text);
        (String::new(), stderr)
    } else {
        (text, stderr)
    }
}

impl From<anyhow::Error> for Output {
    /// A failed command: a fail envelope, and the exit code for the error kind. A library error
    /// takes its text with the command to run next; a library usage error becomes a clap
    /// error, so it is worded as clap words its own.
    fn from(e: anyhow::Error) -> Self {
        let (e, code) = match e.downcast_ref::<owo_core::Error>() {
            Some(err) => {
                let text = hinted(err);
                match err.kind {
                    ErrorKind::Usage => (
                        crate::cli::command(&owo_core::operations())
                            .error(clap::error::ErrorKind::MissingRequiredArgument, text)
                            .into(),
                        exit::USAGE,
                    ),
                    ErrorKind::Config => (anyhow::anyhow!(text), exit::CONFIG),
                    ErrorKind::Io => (anyhow::anyhow!(text), exit::IO),
                    ErrorKind::Failed => (anyhow::anyhow!(text), exit::PROBLEMS),
                }
            }
            None if e.downcast_ref::<clap::Error>().is_some() => (e, exit::USAGE),
            None if e.downcast_ref::<std::io::Error>().is_some() => (e, exit::IO),
            None => (e, exit::PROBLEMS),
        };
        let envelope = Envelope::fail(&format!("{e:#}"));
        Self {
            envelope: serde_json::to_value(envelope).expect("envelope serializes"),
            text: String::new(),
            warnings: vec![],
            error: Some(e),
            code,
        }
    }
}

/// The text of a library error with what to run next, worded with `owo` commands, such as
/// "run `owo add project`". The core's text names no command.
fn hinted(e: &owo_core::Error) -> String {
    let Some(fault) = &e.fault else {
        return e.to_string();
    };
    let hinted = match fault {
        Fault::NeedsTitle(kind) => format!("`owo add {}` needs a title", kind.name()),
        Fault::UnknownTemplate(_) => format!("{fault}; run `owo templates` for the list"),
        Fault::NoIdentityFolder(_) => {
            format!("{fault}; `owo list` shows the identities in this root")
        }
        Fault::NoProject(_) => format!("{fault} (run `owo add project`)"),
        Fault::Generated(_) => format!("{fault}; run `owo index` instead"),
        Fault::NoSoulState => format!("{fault}; run `owo stamp`"),
        Fault::SessionBusy { session, lock } => format!(
            "session {session:?} is busy: another `owo sync` holds {}",
            lock.display()
        ),
        Fault::PushedSession(_) => format!("{fault}; use `owo sync push`"),
        Fault::TranscriptSession { .. } => format!("{fault}; use `owo sync start`"),
        Fault::NoBookmark(_) => format!("{fault}; run `owo sync start` first"),
        Fault::PastMessages { until, seen } => {
            format!("--until {until} is past the {seen} messages the last `sync start` read")
        }
        // Clap refuses these before the call, with its own text.
        Fault::OutOfRange { .. } | Fault::Conflict(..) | Fault::Needs(..) => fault.to_string(),
        Fault::InjectNeedsIdentity => "--inject needs an identity in the root".into(),
    };
    // The text may carry a context before the fault; keep it.
    e.to_string().replace(&fault.to_string(), &hinted)
}
