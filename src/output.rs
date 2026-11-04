//! Output envelope shared by every command: text for people, JSON for agents.

use serde::Serialize;
use serde_json::Value;

/// Exit codes from the design: 0 ok, 1 problems found, 2 usage, 3 config, 4 I/O.
pub mod exit {
    pub const OK: i32 = 0;
    pub const PROBLEMS: i32 = 1;
    pub const CONFIG: i32 = 3;
    pub const IO: i32 = 4;
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
    pub fn ok(data: Value) -> Self {
        Self {
            ok: true,
            data,
            warnings: vec![],
            errors: vec![],
        }
    }

    pub fn fail(message: &str) -> Self {
        Self {
            ok: false,
            data: Value::Null,
            warnings: vec![],
            errors: vec![Value::String(message.to_string())],
        }
    }

    pub fn print(&self) {
        println!(
            "{}",
            serde_json::to_string_pretty(self).expect("envelope serializes")
        );
    }
}
