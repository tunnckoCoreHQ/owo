use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
    Info,
}

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
}

pub fn warning_line(text: &str) -> String {
    format!("owo: warning: {text}\n")
}
