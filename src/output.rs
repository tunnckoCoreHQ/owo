//! Output envelope shared by every command: text for people, JSON for agents.

use serde::Serialize;
use serde_json::Value;

/// Exit codes from the design: 0 ok, 1 problems found, 2 usage, 3 config, 4 I/O.
pub mod exit {
    pub const OK: i32 = 0;
    pub const PROBLEMS: i32 = 1;
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
    pub fn new(
        rule: &str,
        level: Level,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            rule: rule.to_string(),
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

    pub fn print(&self) {
        println!(
            "{}",
            serde_json::to_string_pretty(self).expect("envelope serializes")
        );
    }
}

/// Prints a warning to stderr with the one prefix every command uses.
pub fn warn(message: &str) {
    eprintln!("owo: warning: {message}");
}

/// Where a command's result goes: the JSON envelope, or text for people.
pub struct Out {
    pub json: bool,
    pub quiet: bool,
}

impl Out {
    /// Prints a text line unless `--json` or `--quiet` is set.
    pub fn say(&self, text: impl AsRef<str>) {
        if !self.json && !self.quiet {
            println!("{}", text.as_ref());
        }
    }

    /// Prints the envelope with `--json`; else prints its text warnings to stderr and runs
    /// `text`. Returns the exit code for the envelope's `ok`.
    pub fn emit(&self, envelope: Envelope, text: impl FnOnce()) -> anyhow::Result<i32> {
        let code = exit::of(envelope.ok);
        if self.json {
            envelope.print();
        } else {
            for w in envelope.warnings.iter().filter_map(Value::as_str) {
                warn(w);
            }
            text();
        }
        Ok(code)
    }
}
