//! The output of one command from its typed outcome: the core's envelope, and the text for
//! people from `text`.

use crate::output::{exit, Output};
use crate::text;
use owo_core::{Kind, Level, Outcome, Ran};

/// The output of a run. A failure keeps the warnings found before it. Without `json` the
/// envelope's text warnings join the warnings, and `quiet` drops the confirmation lines.
pub fn output(ran: Ran, json: bool, quiet: bool) -> Output {
    let outcome = match ran.result {
        Ok(o) => o,
        Err(e) => {
            return Output {
                warnings: ran.warnings,
                ..Output::from(anyhow::Error::from(e))
            }
        }
    };
    let envelope = outcome.envelope();
    let mut warnings = ran.warnings;
    let mut text = String::new();
    if !json {
        warnings.extend(envelope.text_warnings());
        let mut t = Text {
            quiet,
            buf: String::new(),
        };
        write(&mut t, &outcome);
        text = t.buf;
    }
    // A name that does not resolve and a search with no hits also print an error line.
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

/// The text result of one command.
struct Text {
    quiet: bool,
    buf: String,
}

impl Text {
    /// Adds a confirmation line unless `--quiet` is set.
    fn say(&mut self, text: impl AsRef<str>) {
        if !self.quiet {
            self.line(text);
        }
    }

    /// Adds a line.
    fn line(&mut self, text: impl AsRef<str>) {
        self.push(text);
        self.buf.push('\n');
    }

    /// Adds text as it is.
    fn push(&mut self, text: impl AsRef<str>) {
        self.buf.push_str(text.as_ref());
    }
}

fn write(t: &mut Text, outcome: &Outcome) {
    match outcome {
        Outcome::Slug(value) => t.line(value),
        Outcome::Config(r) => {
            t.line(format!("root: {} ({:?})", r.root.display(), r.root_source));
            t.line(format!(
                "identity: {}",
                r.identity.as_deref().unwrap_or("-")
            ));
            t.line(format!("project: {}", r.project.as_deref().unwrap_or("-")));
        }
        Outcome::Created(c) => {
            t.say(format!(
                "created {} ({} files)",
                c.dir.display(),
                c.files.len()
            ));
            for f in &c.files {
                t.say(format!("  {f}"));
            }
        }
        Outcome::List { rows, .. } => t.push(text::list(rows)),
        Outcome::Lint { identity, found } => {
            let count = |level: Level| found.iter().filter(|d| d.level == level).count();
            let (errors, warnings) = (count(Level::Error), count(Level::Warning));
            let quiet = t.quiet;
            for d in found.iter().filter(|d| !quiet || d.level == Level::Error) {
                t.line(text::diagnostic(d));
            }
            t.say(format!("{identity}: {errors} errors, {warnings} warnings"));
        }
        Outcome::Write { path, .. } => t.say(path),
        Outcome::Add {
            kind,
            path,
            created,
        } => {
            if *created {
                t.say(path);
            } else if *kind == Kind::Log {
                t.say(format!("exists {path}"));
            } else {
                t.line(format!("exists {path}"));
            }
        }
        Outcome::Stamp { report, .. } => {
            for (label, list) in [
                ("added", &report.added),
                ("changed", &report.changed),
                ("removed", &report.removed),
            ] {
                for p in list {
                    t.say(format!("{label} {p}"));
                }
            }
            if report.is_clean() {
                t.say("state is up to date");
            }
        }
        Outcome::Index { check, paths } => {
            let label = if *check { "stale " } else { "wrote " };
            for p in paths {
                t.say(format!("{label}{p}"));
            }
            if paths.is_empty() {
                t.say("generated files are up to date");
            }
        }
        Outcome::Resolve { found, .. } => {
            if let Ok(hit) = found {
                t.line(&hit.path);
            }
        }
        Outcome::Show { text, .. } | Outcome::Context { text, .. } => t.push(text),
        Outcome::Orphans(hits) => {
            for h in hits {
                t.line(text::hit(h));
            }
        }
        Outcome::Links(l) => t.push(text::links(&l.inbound, &l.outbound)),
        Outcome::State(s) => t.push(text::state(s)),
        Outcome::Search(hits) => t.push(text::search(hits)),
        Outcome::Synced(s) => {
            if let Some(n) = s.added {
                t.say(format!("added {n}"));
            }
            if s.source.is_some() {
                t.line(text::sync(s));
            } else {
                t.say(text::sync(s));
            }
        }
        Outcome::SyncDone { session, until, .. } => {
            t.say(format!("session {session} synced until {until}"))
        }
        Outcome::SyncDue(list) => {
            for d in list {
                t.line(format!("{} {}-{} {}", d.session, d.from, d.until, d.prompt));
            }
        }
        Outcome::Migrate(r) => {
            for (a, b) in &r.moved {
                t.say(format!("{a} -> {b}"));
            }
            t.push(text::migrate(r));
        }
        Outcome::Review(p) => t.push(text::review(p)),
        Outcome::ReviewDone(review) => {
            let through = review.reviewed_through.as_deref().unwrap_or("-");
            t.say(format!("review recorded through {through}"))
        }
        Outcome::SoulNote(note) => t.say(format!(
            "noted {}: {}",
            note.record.file.as_deref().unwrap_or(""),
            note.record.change.as_deref().unwrap_or("")
        )),
        Outcome::SoulHistory(list) => {
            t.push(text::soul_history(list));
            if list.is_empty() {
                t.say("no soul changes recorded");
            }
        }
        Outcome::Changes(c) => t.push(text::changes(c)),
        Outcome::Card { card, out } => match out {
            Some(f) => t.say(format!("wrote {}", f.display())),
            None => t.line(serde_json::to_string_pretty(&card.card).expect("a card serializes")),
        },
        Outcome::Skill(path) => t.say(format!("wrote {path}")),
        Outcome::Soulspec { report, .. } => {
            for f in &report.files {
                t.say(format!("wrote {f}"));
            }
        }
        Outcome::Templates(list) => {
            for tpl in list {
                match &tpl.override_path {
                    Some(p) => t.line(format!("{:<28} override {}", tpl.name, p.display())),
                    None => t.line(format!("{:<28} built-in", tpl.name)),
                }
            }
        }
        Outcome::Template { text, .. } => t.push(text),
    }
}
