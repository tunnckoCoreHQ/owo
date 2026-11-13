//! Output envelope shared by every command: text for people, JSON for agents.

use serde::Serialize;
use serde_json::Value;

/// Exit codes from the design: 0 ok, 1 problems found, 2 usage, 3 config, 4 I/O.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
    Info,
}

/// One finding from a command, for example a lint result.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub rule: String,
    pub level: Level,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
}

impl Diagnostic {
    /// A finding with no rule name yet; the lint runner sets it.
    pub fn new(level: Level, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule: String::new(),
            level,
            path: path.into(),
            line: None,
            message: message.into(),
        }
    }

    pub fn at(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    pub fn render(&self) -> String {
        let level = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Info => "info",
        };
        let place = match self.line {
            Some(line) => format!("{}:{}", self.path, line),
            None => self.path.clone(),
        };
        format!("{level}[{}] {place}: {}", self.rule, self.message)
    }
}

/// The JSON shape every command prints with `--json`.
#[derive(Debug, Serialize)]
pub struct Envelope {
    pub ok: bool,
    pub data: Value,
    pub warnings: Vec<Value>,
    pub errors: Vec<Value>,
}

impl Envelope {
    pub fn new(ok: bool, data: Value) -> Self {
        Self {
            ok,
            data,
            warnings: vec![],
            errors: vec![],
        }
    }

    pub fn ok(data: Value) -> Self {
        Self::new(true, data)
    }

    pub fn fail(message: &str) -> Self {
        Self::new(false, Value::Null).with_errors([message])
    }

    pub fn with_warnings(mut self, items: impl IntoIterator<Item = impl Into<Value>>) -> Self {
        self.warnings.extend(items.into_iter().map(Into::into));
        self
    }

    pub fn with_errors(mut self, items: impl IntoIterator<Item = impl Into<Value>>) -> Self {
        self.errors.extend(items.into_iter().map(Into::into));
        self
    }
}

/// What one command produced. An adapter prints it or returns it.
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
pub fn usage_text(usage: &clap::Error, mut stderr: String) -> (String, String) {
    let text = usage.render().to_string();
    if usage.use_stderr() {
        stderr.push_str(&text);
        (String::new(), stderr)
    } else {
        (text, stderr)
    }
}

impl From<anyhow::Error> for Output {
    /// A failed command: a fail envelope, and the exit code for the error kind.
    fn from(e: anyhow::Error) -> Self {
        let code = if e.downcast_ref::<clap::Error>().is_some() {
            exit::USAGE
        } else if e.downcast_ref::<crate::config::ConfigError>().is_some() {
            exit::CONFIG
        } else if e.downcast_ref::<std::io::Error>().is_some() {
            exit::IO
        } else {
            exit::PROBLEMS
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

/// How a command's result is collected: the envelope with `--json`, else text for people.
pub struct Out {
    pub json: bool,
    pub quiet: bool,
    /// Warnings found before the result, such as a config warning.
    pub warnings: Vec<String>,
}

/// The text result of one command.
pub struct Text {
    quiet: bool,
    buf: String,
}

impl Text {
    /// Adds a confirmation line unless `--quiet` is set.
    pub fn say(&mut self, text: impl AsRef<str>) {
        if !self.quiet {
            self.line(text);
        }
    }

    /// Adds a line.
    pub fn line(&mut self, text: impl AsRef<str>) {
        self.push(text);
        self.buf.push('\n');
    }

    /// Adds text as it is.
    pub fn push(&mut self, text: impl AsRef<str>) {
        self.buf.push_str(text.as_ref());
    }
}

impl Out {
    /// The output for `envelope`. Without `--json` it adds the envelope's text warnings and
    /// runs `text`. The exit code follows the envelope's `ok`.
    pub fn emit(
        &mut self,
        envelope: Envelope,
        text: impl FnOnce(&mut Text),
    ) -> anyhow::Result<Output> {
        let mut warnings = std::mem::take(&mut self.warnings);
        let mut t = Text {
            quiet: self.quiet,
            buf: String::new(),
        };
        if !self.json {
            warnings.extend(
                envelope
                    .warnings
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string),
            );
            text(&mut t);
        }
        Ok(Output {
            code: exit::of(envelope.ok),
            envelope: serde_json::to_value(envelope)?,
            text: t.buf,
            warnings,
            error: None,
        })
    }
}
