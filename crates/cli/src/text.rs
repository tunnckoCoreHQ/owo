use owo_core::{
    day, join_links, now_block, Diagnostic, Hit, Kind, Level, Outcome, Packet, SyncRun,
};

struct Text {
    quiet: bool,
    buf: String,
}

pub fn of(outcome: &Outcome, quiet: bool) -> String {
    let mut t = Text {
        quiet,
        buf: String::new(),
    };
    t.write(outcome);
    t.buf
}

pub fn hit(h: &Hit) -> String {
    if h.summary.is_empty() {
        format!("{}  {}", h.path, h.title)
    } else {
        format!("{}  {}: {}", h.path, h.title, h.summary)
    }
}

fn diagnostic(d: &Diagnostic) -> String {
    let level = match d.level {
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Info => "info",
    };
    let place = match d.line {
        Some(line) => format!("{}:{}", d.path, line),
        None => d.path.clone(),
    };
    format!("{level}[{}] {place}: {}", d.rule, d.message)
}

fn sync(s: &SyncRun) -> String {
    match &s.source {
        Some(src) => format!(
            "due {} {}-{}\nsource {src}\nprompt {}",
            s.identity,
            s.from,
            s.until,
            s.prompt.as_deref().unwrap_or("")
        ),
        None => format!(
            "not due: {} new user turns (every {}){}",
            s.new_user_turns,
            s.every,
            s.pending
                .as_deref()
                .map(|p| format!("; a run is pending ({p}) until `sync done`"))
                .unwrap_or_default()
        ),
    }
}

impl Text {
    fn say(&mut self, text: impl AsRef<str>) {
        if !self.quiet {
            self.line(text);
        }
    }

    fn line(&mut self, text: impl AsRef<str>) {
        self.push(text);
        self.buf.push('\n');
    }

    fn push(&mut self, text: impl AsRef<str>) {
        self.buf.push_str(text.as_ref());
    }

    fn write(&mut self, outcome: &Outcome) {
        match outcome {
            Outcome::Slug(value) => self.line(value),
            Outcome::Config(r) => {
                self.line(format!("root: {} ({:?})", r.root.display(), r.root_source));
                self.line(format!(
                    "identity: {}",
                    r.identity.as_deref().unwrap_or("-")
                ));
                self.line(format!("project: {}", r.project.as_deref().unwrap_or("-")));
            }
            Outcome::Created(c) => {
                self.say(format!(
                    "created {} ({} files)",
                    c.dir.display(),
                    c.files.len()
                ));
                for f in &c.files {
                    self.say(format!("  {f}"));
                }
            }
            Outcome::List { rows, .. } => {
                for row in rows {
                    let last = row.last_activity.as_deref().map(day).unwrap_or("-");
                    self.line(format!(
                        "{:<16} {:<20} {:<10} {}",
                        row.slug,
                        row.name,
                        last,
                        row.projects.join(", ")
                    ));
                }
            }
            Outcome::Lint { identity, found } => {
                let count = |level: Level| found.iter().filter(|d| d.level == level).count();
                let (errors, warnings) = (count(Level::Error), count(Level::Warning));
                for d in found {
                    if d.level == Level::Error {
                        self.line(diagnostic(d));
                    } else {
                        self.say(diagnostic(d));
                    }
                }
                self.say(format!("{identity}: {errors} errors, {warnings} warnings"));
            }
            Outcome::Write { path, .. } => self.say(path),
            Outcome::Add {
                kind,
                path,
                created,
            } => {
                if *created {
                    self.say(path);
                } else if *kind == Kind::Log {
                    self.say(format!("exists {path}"));
                } else {
                    self.line(format!("exists {path}"));
                }
            }
            Outcome::Stamp { report, .. } => {
                for (label, list) in [
                    ("added", &report.added),
                    ("changed", &report.changed),
                    ("removed", &report.removed),
                ] {
                    for p in list {
                        self.say(format!("{label} {p}"));
                    }
                }
                if report.is_clean() {
                    self.say("state is up to date");
                }
            }
            Outcome::Index { check, paths } => {
                let label = if *check { "stale " } else { "wrote " };
                for p in paths {
                    self.say(format!("{label}{p}"));
                }
                if paths.is_empty() {
                    self.say("generated files are up to date");
                }
            }
            Outcome::Resolve { found, .. } => {
                if let Ok(hit) = found {
                    self.line(&hit.path);
                }
            }
            Outcome::Show { text, .. }
            | Outcome::Context { text, .. }
            | Outcome::Soul { text, .. }
            | Outcome::Template { text, .. } => self.push(text),
            Outcome::Orphans(hits) => {
                for h in hits {
                    self.line(hit(h));
                }
            }
            Outcome::Links(l) => {
                for r in &l.inbound {
                    self.line(format!("in  {}:{}  {}", r.path, r.line, r.text));
                }
                for r in &l.outbound {
                    let to = if r.path.is_empty() {
                        "(broken)"
                    } else {
                        &r.path
                    };
                    self.line(format!("out [[{}]] -> {to}  (line {})", r.target, r.line));
                }
            }
            Outcome::State(s) => {
                self.line(format!("# {}\n", s.identity));
                self.push(now_block(s, None));
                if let Some(change) = &s.last_soul_change {
                    self.line(format!(
                        "- Last soul change: {} {}",
                        change.file.as_deref().unwrap_or("?"),
                        change.change.as_deref().unwrap_or("")
                    ));
                }
                if let Some(p) = &s.project {
                    let last = p.last_updated.as_deref().map(day).unwrap_or("never");
                    self.line(format!(
                        "- Project {}: last {last}, notes {}",
                        p.slug,
                        join_links(&p.notes)
                    ));
                }
                if s.review.memory_changed_outside_review {
                    self.line("- MEMORY.md changed outside a review");
                }
            }
            Outcome::Search(hits) => {
                for h in hits {
                    self.line(format!("{:>6.2}  {}  {}", h.score, h.path, h.title));
                    if let (Some(l), Some(s)) = (h.line, &h.snippet) {
                        self.line(format!("        {l}: {s}"));
                    }
                }
            }
            Outcome::Synced(s) => {
                if let Some(n) = s.added {
                    self.say(format!("added {n}"));
                }
                if s.source.is_some() {
                    self.line(sync(s));
                } else {
                    self.say(sync(s));
                }
            }
            Outcome::SyncDone { session, until, .. } => {
                self.say(format!("session {session} synced until {until}"))
            }
            Outcome::SyncDue(list) => {
                for d in list {
                    self.line(format!("{} {}-{} {}", d.session, d.from, d.until, d.prompt));
                }
            }
            Outcome::Migrate(r) => {
                for (a, b) in &r.moved {
                    self.say(format!("{a} -> {b}"));
                }
                for s in &r.skipped {
                    self.line(format!("skipped {s}"));
                }
                self.line(format!("\nChecklist for {}:", r.dir));
                for c in &r.checklist {
                    self.line(format!("- [ ] {c}"));
                }
            }
            Outcome::Review(p) => self.review(p),
            Outcome::ReviewDone(review) => {
                let through = review.reviewed_through.as_deref().unwrap_or("-");
                self.say(format!("review recorded through {through}"))
            }
            Outcome::SoulNote(note) => self.say(format!(
                "noted {}: {}",
                note.record.file.as_deref().unwrap_or(""),
                note.record.change.as_deref().unwrap_or("")
            )),
            Outcome::SoulHistory(list) => {
                for r in list {
                    let loose = r.other.get("evidence").and_then(|v| v.as_array());
                    let evidence: Vec<&str> = (r.evidence.iter().flatten().map(String::as_str))
                        .chain(loose.into_iter().flatten().filter_map(|v| v.as_str()))
                        .collect();
                    self.line(format!(
                        "{}  {}  {}\n    evidence: {}",
                        r.at.as_deref().unwrap_or("?"),
                        r.file.as_deref().unwrap_or("?"),
                        r.change.as_deref().unwrap_or(""),
                        evidence.join(", ")
                    ));
                }
                if list.is_empty() {
                    self.say("no soul changes recorded");
                }
            }
            Outcome::Changes(c) => {
                for (d, list) in c.days.iter().rev() {
                    self.line(d);
                    for ch in list {
                        let commit = ch
                            .commit
                            .as_ref()
                            .map(|h| format!("  {h}"))
                            .unwrap_or_default();
                        self.line(format!("  {:<5} {}{commit}", ch.status, ch.path));
                    }
                }
                if !c.pending.is_empty() {
                    self.line("uncommitted");
                    for p in &c.pending {
                        self.line(format!("  {p}"));
                    }
                }
                if self.buf.is_empty() {
                    self.line("no changes");
                }
            }
            Outcome::Card { card, out } => match out {
                Some(f) => self.say(format!("wrote {}", f.display())),
                None => {
                    self.line(serde_json::to_string_pretty(&card.card).expect("a card serializes"))
                }
            },
            Outcome::Skill(path) => self.say(format!("wrote {path}")),
            Outcome::Soulspec { report, .. } => {
                for f in &report.files {
                    self.say(format!("wrote {f}"));
                }
            }
            Outcome::Templates(list) => {
                for tpl in list {
                    match &tpl.override_path {
                        Some(p) => self.line(format!("{:<28} override {}", tpl.name, p.display())),
                        None => self.line(format!("{:<28} built-in", tpl.name)),
                    }
                }
            }
            Outcome::DryRun { outcome, writes } => {
                let quiet = std::mem::replace(&mut self.quiet, true);
                self.write(outcome);
                self.quiet = quiet;
                for w in writes {
                    self.line(format!("would write {w}"));
                }
                if writes.is_empty() {
                    self.line("would write nothing");
                }
            }
        }
    }

    fn review(&mut self, p: &Packet) {
        self.line(format!("# Review packet: {}\n", p.identity));
        self.line(format!(
            "Last reviewed: {}. Reviewed through: {}.\n",
            p.last_reviewed.as_deref().unwrap_or("never"),
            p.reviewed_through.as_deref().unwrap_or("-")
        ));
        let outside = if p.memory_changed_outside_review {
            ", changed outside a review"
        } else {
            ""
        };
        self.line(format!(
            "## Current MEMORY ({} words{outside})\n\n{}\n",
            p.memory_words,
            p.memory.trim_end()
        ));
        self.line(format!(
            "## Changed since last review ({})\n",
            p.changed.len()
        ));
        for c in &p.changed {
            let git = c
                .git
                .map(|(a, d)| format!(", git +{a} -{d}"))
                .unwrap_or_default();
            self.line(format!(
                "- {} ({}, {} words{git}): {}",
                c.path,
                c.kind,
                c.words,
                c.summary.as_deref().unwrap_or("-")
            ));
        }
        self.line(format!(
            "\n## Unreviewed days ({})",
            p.unreviewed_days.len()
        ));
        for d in &p.unreviewed_days {
            let heading = if d.title == d.date {
                d.date.clone()
            } else {
                format!("{} {}", d.date, d.title)
            };
            self.line(format!("\n### {heading}\n\n{}", d.body));
        }
        self.line(format!(
            "\n## Events in window\n\n{}",
            join_links(&p.events)
        ));
        self.line(format!(
            "\n## Lessons changed\n\n{}",
            join_links(&p.lessons)
        ));
        let journal = if p.journal.is_empty() {
            "none".into()
        } else {
            p.journal.join(", ")
        };
        self.line(format!("\n## Journal in window\n\n{journal}"));
        self.line(format!(
            "\n## Open\n\n- Threads: {}\n- Conflicts: {}",
            join_links(&p.open_threads),
            join_links(&p.open_conflicts)
        ));
        if !p.recurring.is_empty() {
            self.line("\n## Recurring phrases\n");
            for (phrase, n) in &p.recurring {
                self.line(format!("- \"{phrase}\" on {n} days"));
            }
        }
        if let Some(c) = &p.last_soul_change {
            self.line(format!(
                "\n## Last soul change\n\n{} {}: {}",
                c.at.as_deref().unwrap_or("?"),
                c.file.as_deref().unwrap_or("?"),
                c.change.as_deref().unwrap_or("")
            ));
        }
        self.line("\nWhen MEMORY is compiled: `owo stamp && owo index && owo lint`, then `owo review done`.");
    }
}
