//! The text each command prints without `--json`, built from the typed results of the library.

use owo::{
    Changes, Context, Diagnostic, Hit, Level, LinkRef, MigrateReport, Packet, Row, SearchHit,
    State, SyncRun,
};
use serde_json::Value;

/// One finding as `level[rule] path:line: message`.
pub fn diagnostic(d: &Diagnostic) -> String {
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

pub fn list(rows: &[Row]) -> String {
    rows.iter()
        .map(|row| {
            let last = row.last_activity.as_deref().map(owo::day).unwrap_or("-");
            format!(
                "{:<16} {:<20} {:<10} {}\n",
                row.slug,
                row.name,
                last,
                row.projects.join(", ")
            )
        })
        .collect()
}

pub fn state(s: &State) -> String {
    let mut out = format!("# {}\n\n", s.identity);
    out.push_str(&owo::now_block(s, None));
    if let Some(change) = &s.last_soul_change {
        out.push_str(&format!(
            "- Last soul change: {} {}\n",
            change.get("file").and_then(Value::as_str).unwrap_or("?"),
            change.get("change").and_then(Value::as_str).unwrap_or("")
        ));
    }
    if let Some(p) = &s.project {
        let last = p.last_updated.as_deref().map(owo::day).unwrap_or("never");
        out.push_str(&format!(
            "- Project {}: last {last}, notes {}\n",
            p.slug,
            owo::join_links(&p.notes)
        ));
    }
    if s.review.memory_changed_outside_review {
        out.push_str("- MEMORY.md changed outside a review\n");
    }
    out
}

pub fn context_paths(c: &Context) -> String {
    let mut out = String::new();
    // The Now block follows the core files, as in the text format.
    let core = c.items.iter().take_while(|it| it.tier == "core").count();
    for (n, it) in c.items.iter().enumerate() {
        if n == core {
            out.push_str("now      (generated block)\n");
        }
        out.push_str(&format!("{:<8} {}  ({})\n", it.tier, it.path, it.reason));
    }
    if core == c.items.len() {
        out.push_str("now      (generated block)\n");
    }
    if let Some(kb) = &c.kb {
        out.push_str(&format!(
            "kb       {}{}\n",
            kb.path,
            if kb.exists { "" } else { "  (missing)" }
        ));
    }
    out
}

pub fn search(hits: &[SearchHit]) -> String {
    let mut out = String::new();
    for h in hits {
        out.push_str(&format!("{:>6.2}  {}  {}\n", h.score, h.path, h.title));
        if let (Some(l), Some(s)) = (h.line, &h.snippet) {
            out.push_str(&format!("        {l}: {s}\n"));
        }
    }
    out
}

/// The due run (slice and prompt), or why no run is due.
pub fn sync(s: &SyncRun) -> String {
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

/// The skipped files and the checklist; `moved` lines are left to the caller, which drops them
/// under `--quiet`.
pub fn migrate(r: &MigrateReport) -> String {
    let mut out = String::new();
    for s in &r.skipped {
        out.push_str(&format!("skipped {s}\n"));
    }
    out.push_str(&format!("\nChecklist for {}:\n", r.dir));
    for c in &r.checklist {
        out.push_str(&format!("- [ ] {c}\n"));
    }
    out
}

pub fn review(p: &Packet) -> String {
    let mut o = format!("# Review packet: {}\n\n", p.identity);
    o.push_str(&format!(
        "Last reviewed: {}. Reviewed through: {}.\n\n",
        p.last_reviewed.as_deref().unwrap_or("never"),
        p.reviewed_through.as_deref().unwrap_or("-")
    ));
    o.push_str(&format!(
        "## Current MEMORY ({} words{})\n\n{}\n\n",
        p.memory_words,
        if p.memory_changed_outside_review {
            ", changed outside a review"
        } else {
            ""
        },
        p.memory.trim_end()
    ));
    o.push_str(&format!(
        "## Changed since last review ({})\n\n",
        p.changed.len()
    ));
    for c in &p.changed {
        let git = c
            .git
            .map(|(a, d)| format!(", git +{a} -{d}"))
            .unwrap_or_default();
        o.push_str(&format!(
            "- {} ({}, {} words{git}): {}\n",
            c.path,
            c.kind,
            c.words,
            c.summary.as_deref().unwrap_or("-")
        ));
    }
    o.push_str(&format!(
        "\n## Unreviewed days ({})\n",
        p.unreviewed_days.len()
    ));
    for d in &p.unreviewed_days {
        let heading = if d.title == d.date {
            d.date.clone()
        } else {
            format!("{} {}", d.date, d.title)
        };
        o.push_str(&format!("\n### {heading}\n\n{}\n", d.body));
    }
    o.push_str(&format!(
        "\n## Events in window\n\n{}\n",
        owo::join_links(&p.events)
    ));
    o.push_str(&format!(
        "\n## Lessons changed\n\n{}\n",
        owo::join_links(&p.lessons)
    ));
    o.push_str(&format!(
        "\n## Journal in window\n\n{}\n",
        if p.journal.is_empty() {
            "none".into()
        } else {
            p.journal.join(", ")
        }
    ));
    o.push_str(&format!(
        "\n## Open\n\n- Threads: {}\n- Conflicts: {}\n",
        owo::join_links(&p.open_threads),
        owo::join_links(&p.open_conflicts)
    ));
    if !p.recurring.is_empty() {
        o.push_str("\n## Recurring phrases\n\n");
        for (phrase, n) in &p.recurring {
            o.push_str(&format!("- \"{phrase}\" on {n} days\n"));
        }
    }
    if let Some(c) = &p.last_soul_change {
        o.push_str(&format!(
            "\n## Last soul change\n\n{} {}: {}\n",
            c["at"].as_str().unwrap_or("?"),
            c["file"].as_str().unwrap_or("?"),
            c["change"].as_str().unwrap_or("")
        ));
    }
    o.push_str("\nWhen MEMORY is compiled: `owo stamp && owo index && owo lint`, then `owo review done`.\n");
    o
}

pub fn soul_history(list: &[Value]) -> String {
    let mut out = String::new();
    for r in list {
        let ev: Vec<&str> = r["evidence"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        out.push_str(&format!(
            "{}  {}  {}\n    evidence: {}\n",
            r["at"].as_str().unwrap_or("?"),
            r["file"].as_str().unwrap_or("?"),
            r["change"].as_str().unwrap_or(""),
            ev.join(", ")
        ));
    }
    out
}

pub fn changes(c: &Changes) -> String {
    let mut o = String::new();
    for (day, list) in c.days.iter().rev() {
        o.push_str(&format!("{day}\n"));
        for ch in list {
            let commit = ch
                .commit
                .as_ref()
                .map(|h| format!("  {h}"))
                .unwrap_or_default();
            o.push_str(&format!("  {:<5} {}{commit}\n", ch.status, ch.path));
        }
    }
    if !c.pending.is_empty() {
        o.push_str("uncommitted\n");
        for p in &c.pending {
            o.push_str(&format!("  {p}\n"));
        }
    }
    if o.is_empty() {
        o.push_str("no changes\n");
    }
    o
}

pub fn hit(h: &Hit) -> String {
    if h.summary.is_empty() {
        format!("{}  {}", h.path, h.title)
    } else {
        format!("{}  {}: {}", h.path, h.title, h.summary)
    }
}

pub fn links(back: &[LinkRef], fwd: &[LinkRef]) -> String {
    let mut out = String::new();
    for l in back {
        out.push_str(&format!("in  {}:{}  {}\n", l.path, l.line, l.text));
    }
    for l in fwd {
        let to = if l.path.is_empty() {
            "(broken)"
        } else {
            &l.path
        };
        out.push_str(&format!(
            "out [[{}]] -> {to}  (line {})\n",
            l.target, l.line
        ));
    }
    out
}
