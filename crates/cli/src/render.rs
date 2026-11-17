use crate::output::{exit, Output};
use crate::text;
use owo_core::{Outcome, Result};

pub fn output(result: Result<Outcome>, json: bool, quiet: bool) -> Output {
    let outcome = match result {
        Ok(o) => o,
        Err(e) => return Output::from(anyhow::Error::from(e)),
    };
    let envelope = outcome.envelope();
    let mut warnings = vec![];
    let mut text = String::new();
    if !json {
        warnings.extend(envelope.text_warnings());
        text = text::of(&outcome, quiet);
    }
    let error = match &outcome {
        Outcome::Resolve {
            found: Err(hits), ..
        } => {
            let msg = envelope.errors[0].as_str().unwrap_or_default();
            let lines: String = hits
                .iter()
                .map(|h| format!("\n  {}", text::hit(h)))
                .collect();
            Some(anyhow::anyhow!("{msg}{lines}"))
        }
        Outcome::Search(hits) if hits.is_empty() => Some(anyhow::anyhow!("no hits")),
        _ => None,
    };
    Output {
        code: exit::of(envelope.ok),
        envelope: serde_json::to_value(envelope).expect("envelope serializes"),
        text,
        warnings,
        error,
    }
}
