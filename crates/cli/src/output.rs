use owo_core::{Envelope, ErrorKind, Fault, Named};
use serde_json::Value;

pub mod exit {
    pub const OK: i32 = 0;
    pub const PROBLEMS: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const CONFIG: i32 = 3;
    pub const IO: i32 = 4;

    pub fn of(ok: bool) -> i32 {
        if ok {
            OK
        } else {
            PROBLEMS
        }
    }
}

pub struct Output {
    pub envelope: Value,
    pub text: String,
    pub warnings: Vec<String>,
    pub error: Option<anyhow::Error>,
    pub code: i32,
}

impl Output {
    pub fn warning_lines(&self) -> String {
        self.warnings
            .iter()
            .map(|w| owo_core::warning_line(w))
            .collect()
    }

    pub fn usage(&self) -> Option<&anyhow::Error> {
        self.error.as_ref().filter(|_| self.code == exit::USAGE)
    }

    pub fn render(&self, json: bool) -> (String, String) {
        let mut stderr = self.warning_lines();
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

impl From<anyhow::Error> for Output {
    fn from(e: anyhow::Error) -> Self {
        let (e, code) = match e.downcast_ref::<owo_core::Error>() {
            Some(err) => {
                let code = match err.kind {
                    ErrorKind::Usage => exit::USAGE,
                    ErrorKind::Config => exit::CONFIG,
                    ErrorKind::Io => exit::IO,
                    ErrorKind::Failed => exit::PROBLEMS,
                };
                let text = match &err.fault {
                    Some(fault) => says(fault),
                    None => err.to_string(),
                };
                (anyhow::anyhow!(text), code)
            }
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

fn says(fault: &Fault) -> String {
    match fault {
        Fault::NoRoot => format!("{fault}: pass --root or set OWO_ROOT"),
        Fault::NoIdentity(_) => format!("{fault}: pass -i <slug> or set OWO_IDENTITY"),
        Fault::NeedsTitle(kind) => format!("`owo add {}` needs a title", kind.name()),
        Fault::KindNeeds {
            kind,
            field: "parent",
        } => format!("{} needs --parent <location>", kind.name()),
        Fault::KindNeeds { kind, field } => format!("{} needs --{field} <{field}>", kind.name()),
        Fault::TemplateLacks {
            template,
            line,
            field,
        } => format!("the {template} template has no `{line}` line for --{field}"),
        Fault::NoText(_) => format!("{fault}; pass --content or pipe the full file on stdin"),
        Fault::SinceNotDay(e) => format!("outside Git, --since takes a day: {e}"),
        Fault::OnlyFor { .. }
        | Fault::Empty(_)
        | Fault::Field { .. }
        | Fault::OutNoSlug
        | Fault::BehindBookmark { .. }
        | Fault::InjectNeedsIdentity => format!("--{fault}"),
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
        Fault::NoMessages { .. } => format!("{fault}; check --format"),
        Fault::NoBookmark(_) => format!("{fault}; run `owo sync start` first"),
        Fault::PastMessages { until, seen } => {
            format!("--until {until} is past the {seen} messages the last `sync start` read")
        }
        Fault::OutOfRange { .. } | Fault::Conflict(..) | Fault::Needs(..) => fault.to_string(),
        Fault::NoIngestCommand(_) => format!("{fault} (set it in .owo.toml or the user config)"),
    }
}
