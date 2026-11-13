//! The output of each command from its typed library result: the JSON envelope and the text.
//! The CLI and the MCP server both call the library and then these, so a command and a tool
//! give the same output.

use crate::output::{Envelope, Out, Output};
use crate::text;
use anyhow::Result;
use owo::{
    Bookmark, Card, Changes, Context, Created, Diagnostic, Env, Hit, Identity, Kind, Level, Links,
    MigrateReport, Packet, Resolved, ReviewRecord, Root, Row, SearchHit, Select, SoulChange,
    SoulNote, Soulspec, StampReport, State, SyncRun, Template,
};
use serde_json::json;
use std::path::Path;

/// The output of one command run by `f`. A failure keeps the warnings found before it.
pub fn finish(mut out: Out, f: impl FnOnce(&mut Out) -> Result<Output>) -> Output {
    match f(&mut out) {
        Ok(o) => o,
        Err(e) => Output {
            warnings: out.warnings,
            ..Output::from(e)
        },
    }
}

/// The root `select` picks; its config warnings go to `out`.
pub fn open(select: &Select, env: &Env, out: &mut Out) -> Result<Root> {
    let root = Root::open(select, env)?;
    out.warnings.extend(root.warnings.iter().cloned());
    Ok(root)
}

/// The selected identity of the root `select` picks.
pub fn load(select: &Select, env: &Env, out: &mut Out) -> Result<Identity> {
    Ok(open(select, env, out)?.identity()?)
}

pub fn slug(out: &mut Out, value: &str) -> Result<Output> {
    out.emit(Envelope::ok(json!({ "slug": value })), |t| t.line(value))
}

pub fn config(out: &mut Out, r: &Resolved) -> Result<Output> {
    out.emit(Envelope::ok(serde_json::to_value(r)?), |t| {
        t.line(format!("root: {} ({:?})", r.root.display(), r.root_source));
        t.line(format!(
            "identity: {}",
            r.identity.as_deref().unwrap_or("-")
        ));
        t.line(format!("project: {}", r.project.as_deref().unwrap_or("-")));
    })
}

pub fn created(out: &mut Out, c: &Created) -> Result<Output> {
    let data = json!({ "slug": c.slug, "path": c.dir, "files": c.files });
    out.emit(Envelope::ok(data), |t| {
        t.say(format!(
            "created {} ({} files)",
            c.dir.display(),
            c.files.len()
        ));
        for f in &c.files {
            t.say(format!("  {f}"));
        }
    })
}

pub fn list(out: &mut Out, root: &Path, rows: &[Row]) -> Result<Output> {
    out.emit(
        Envelope::ok(json!({ "root": root, "identities": rows })),
        |t| t.push(text::list(rows)),
    )
}

pub fn lint(out: &mut Out, slug: &str, found: &[Diagnostic]) -> Result<Output> {
    let count = |level: Level| found.iter().filter(|d| d.level == level).count();
    let (errors, warnings) = (count(Level::Error), count(Level::Warning));
    let split = |level: Level| {
        found
            .iter()
            .filter(|d| d.level == level)
            .map(|d| serde_json::to_value(d).expect("serializes"))
            .collect::<Vec<_>>()
    };
    let envelope = Envelope::new(
        errors == 0,
        json!({ "identity": slug, "infos": split(Level::Info) }),
    )
    .with_warnings(split(Level::Warning))
    .with_errors(split(Level::Error));
    let quiet = out.quiet;
    out.emit(envelope, |t| {
        for d in found.iter().filter(|d| !quiet || d.level == Level::Error) {
            t.line(text::diagnostic(d));
        }
        t.say(format!("{slug}: {errors} errors, {warnings} warnings"));
    })
}

pub fn write(out: &mut Out, path: &str, created: bool) -> Result<Output> {
    out.emit(
        Envelope::ok(json!({ "path": path, "created": created })),
        |t| t.say(path),
    )
}

pub fn add(out: &mut Out, kind: Kind, path: &str, created: bool) -> Result<Output> {
    let ok = created || kind == Kind::Log;
    let envelope = Envelope::new(ok, json!({ "path": path, "created": created }))
        .with_errors((!ok).then(|| format!("exists: {path}")));
    out.emit(envelope, |t| {
        if created {
            t.say(path);
        } else if ok {
            t.say(format!("exists {path}"));
        } else {
            t.line(format!("exists {path}"));
        }
    })
}

pub fn stamp(out: &mut Out, check: bool, report: &StampReport) -> Result<Output> {
    let clean = report.is_clean();
    let envelope = Envelope::new(!check || clean, serde_json::to_value(report)?);
    out.emit(envelope, |t| {
        for (label, list) in [
            ("added", &report.added),
            ("changed", &report.changed),
            ("removed", &report.removed),
        ] {
            for p in list {
                t.say(format!("{label} {p}"));
            }
        }
        if clean {
            t.say("state is up to date");
        }
    })
}

/// The written generated files, or with `check` the stale ones.
pub fn index(out: &mut Out, check: bool, paths: &[String]) -> Result<Output> {
    let (key, label) = if check {
        ("stale", "stale ")
    } else {
        ("written", "wrote ")
    };
    let envelope = Envelope::new(!check || paths.is_empty(), json!({ key: paths }));
    out.emit(envelope, |t| {
        for p in paths {
            t.say(format!("{label}{p}"));
        }
        if paths.is_empty() {
            t.say("generated files are up to date");
        }
    })
}

pub fn resolve(out: &mut Out, name: &str, found: Result<Hit, Vec<Hit>>) -> Result<Output> {
    match found {
        Ok(hit) => out.emit(Envelope::ok(serde_json::to_value(&hit)?), |t| {
            t.line(&hit.path)
        }),
        Err(hits) => {
            let msg = text::unresolved(name);
            let envelope =
                Envelope::new(false, json!({ "candidates": hits })).with_errors([msg.as_str()]);
            let lines: String = hits
                .iter()
                .map(|h| format!("\n  {}", text::hit(h)))
                .collect();
            let mut o = out.emit(envelope, |_| {})?;
            o.error = Some(anyhow::anyhow!("{msg}{lines}"));
            Ok(o)
        }
    }
}

pub fn show(out: &mut Out, path: &str, text: &str) -> Result<Output> {
    out.emit(Envelope::ok(json!({ "path": path, "text": text })), |t| {
        t.push(text)
    })
}

pub fn orphans(out: &mut Out, hits: &[Hit]) -> Result<Output> {
    out.emit(Envelope::ok(json!({ "orphans": hits })), |t| {
        for h in hits {
            t.line(text::hit(h));
        }
    })
}

pub fn links(out: &mut Out, l: &Links) -> Result<Output> {
    out.emit(
        Envelope::ok(json!({ "path": l.path, "in": l.inbound, "out": l.outbound })),
        |t| t.push(text::links(&l.inbound, &l.outbound)),
    )
}

/// The load set: its text, or with `paths` the file list.
pub fn context(out: &mut Out, id: &Identity, ctx: &Context, paths: bool) -> Result<Output> {
    let envelope = Envelope::ok(serde_json::to_value(ctx)?).with_warnings(ctx.warnings.clone());
    out.emit(envelope, |t| {
        if !paths {
            t.push(id.context_text(ctx));
        } else {
            t.push(text::context_paths(ctx));
        }
    })
}

pub fn state(out: &mut Out, s: &State) -> Result<Output> {
    out.emit(Envelope::ok(serde_json::to_value(s)?), |t| {
        t.push(text::state(s))
    })
}

pub fn search(out: &mut Out, hits: &[SearchHit]) -> Result<Output> {
    let ok = !hits.is_empty();
    let envelope =
        Envelope::new(ok, json!({ "hits": hits })).with_errors((!ok).then_some("no hits"));
    let mut o = out.emit(envelope, |t| t.push(text::search(hits)))?;
    o.error = (!ok).then(|| anyhow::anyhow!("no hits"));
    Ok(o)
}

/// A `sync start` or `sync push` run.
pub fn synced(out: &mut Out, s: &SyncRun) -> Result<Output> {
    out.emit(Envelope::ok(serde_json::to_value(s)?), |t| {
        if let Some(n) = s.added {
            t.say(format!("added {n}"));
        }
        if s.source.is_some() {
            t.line(text::sync(s));
        } else {
            t.say(text::sync(s));
        }
    })
}

pub fn sync_done(out: &mut Out, session: &str, until: usize, b: &Bookmark) -> Result<Output> {
    out.emit(Envelope::ok(serde_json::to_value(b)?), |t| {
        t.say(format!("session {session} synced until {until}"))
    })
}

pub fn migrate(out: &mut Out, r: &MigrateReport) -> Result<Output> {
    out.emit(Envelope::ok(serde_json::to_value(r)?), |t| {
        for (a, b) in &r.moved {
            t.say(format!("{a} -> {b}"));
        }
        t.push(text::migrate(r));
    })
}

pub fn review(out: &mut Out, p: &Packet) -> Result<Output> {
    out.emit(Envelope::ok(serde_json::to_value(p)?), |t| {
        t.push(text::review(p))
    })
}

pub fn review_done(out: &mut Out, review: &ReviewRecord) -> Result<Output> {
    let through = review.reviewed_through.as_deref().unwrap_or("-");
    out.emit(Envelope::ok(serde_json::to_value(review)?), |t| {
        t.say(format!("review recorded through {through}"))
    })
}

pub fn soul_note(out: &mut Out, note: SoulNote) -> Result<Output> {
    let line = format!(
        "noted {}: {}",
        note.record.file.as_deref().unwrap_or(""),
        note.record.change.as_deref().unwrap_or("")
    );
    let envelope = Envelope::ok(serde_json::to_value(&note.record)?).with_warnings(note.warning);
    out.emit(envelope, |t| t.say(line))
}

pub fn soul_history(out: &mut Out, list: &[SoulChange]) -> Result<Output> {
    out.emit(Envelope::ok(json!({ "history": list })), |t| {
        t.push(text::soul_history(list));
        if list.is_empty() {
            t.say("no soul changes recorded");
        }
    })
}

pub fn changes(out: &mut Out, c: &Changes) -> Result<Output> {
    out.emit(Envelope::ok(serde_json::to_value(c)?), |t| {
        t.push(text::changes(c))
    })
}

/// A built card: the card itself, or the file it was written to.
pub fn card(out: &mut Out, c: Card, file: Option<&Path>) -> Result<Output> {
    let Some(f) = file else {
        let text = serde_json::to_string_pretty(&c.card)?;
        return out.emit(Envelope::ok(c.card).with_warnings(c.warnings), |t| {
            t.line(&text)
        });
    };
    out.emit(
        Envelope::ok(json!({ "path": f })).with_warnings(c.warnings),
        |t| t.say(format!("wrote {}", f.display())),
    )
}

pub fn skill(out: &mut Out, path: &str) -> Result<Output> {
    out.emit(Envelope::ok(json!({ "path": path })), |t| {
        t.say(format!("wrote {path}"))
    })
}

pub fn soulspec(out: &mut Out, dir: &Path, report: Soulspec) -> Result<Output> {
    let envelope =
        Envelope::ok(json!({ "dir": dir, "files": report.files })).with_warnings(report.warnings);
    out.emit(envelope, |t| {
        for f in &report.files {
            t.say(format!("wrote {f}"));
        }
    })
}

pub fn templates(out: &mut Out, list: &[Template]) -> Result<Output> {
    out.emit(Envelope::ok(json!({ "templates": list })), |t| {
        for tpl in list {
            match &tpl.override_path {
                Some(p) => t.line(format!("{:<28} override {}", tpl.name, p.display())),
                None => t.line(format!("{:<28} built-in", tpl.name)),
            }
        }
    })
}

pub fn template(out: &mut Out, tpl: &Template, text: &str) -> Result<Output> {
    out.emit(
        Envelope::ok(json!({ "name": tpl.name, "override": tpl.override_path, "text": text })),
        |t| t.push(text),
    )
}
