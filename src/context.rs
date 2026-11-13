//! `owo context`: the tiered load set for an identity.
//! Core (always): IDENTITY, SOUL, MEMORY and the generated "Now" block.
//! Voice: VOICE.md. Look: LOOK.md. Project: work README, project.md, recent notes, KB pointer.

use crate::identity::Identity;
use crate::status;
use anyhow::Result;
use serde::Serialize;
use std::fs;

pub struct Request<'a> {
    pub voice: bool,
    pub look: bool,
    pub project: Option<&'a str>,
    pub with: &'a [String],
    pub notes: usize,
}

#[derive(Debug, Serialize)]
pub struct Item {
    pub tier: &'static str,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct Context {
    pub identity: String,
    pub project: Option<String>,
    pub items: Vec<Item>,
    pub now: String,
    pub kb: Option<crate::kb::Kb>,
    pub warnings: Vec<String>,
}

fn item(tier: &'static str, path: &str, reason: &str) -> Item {
    Item {
        tier,
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

pub fn build(id: &Identity, req: &Request) -> Result<Context> {
    let mut items = vec![
        item("core", "soul/IDENTITY.md", "who the identity is"),
        item("core", "soul/SOUL.md", "first-person core"),
        item("core", "soul/MEMORY.md", "compiled long-term memory"),
    ];
    if req.voice {
        items.push(item(
            "voice",
            "soul/VOICE.md",
            "talking or writing as itself",
        ));
    }
    if req.look {
        items.push(item("look", "soul/LOOK.md", "visuals matter"));
    }
    let mut warnings = vec![];
    let mut kb = None;
    if let Some(p) = req.project {
        let project_md = format!("work/projects/{p}/project.md");
        items.push(item("project", "work/README.md", "work profile"));
        items.push(item("project", &project_md, "role on this project"));
        for n in id.notes(p).into_iter().take(req.notes) {
            items.push(item("project", &n.rel, "recent project note"));
        }
        let (pointer, warning) = crate::kb::pointer(id, p);
        kb = pointer;
        warnings.extend(warning);
    }
    for name in req.with {
        match crate::find::one(id, name) {
            // A file already in the load set is loaded once.
            Ok(i) if items.iter().any(|it| it.path == id.entries[i].rel) => {}
            Ok(i) => items.push(item(
                "with",
                &id.entries[i].rel,
                &format!("requested: {name}"),
            )),
            Err(e) => warnings.push(format!("--with {e}")),
        }
    }
    for it in &items {
        if !id.dir.join(&it.path).is_file() {
            warnings.push(format!("missing {}", it.path));
        }
    }
    let state = status::compute(id, 14, 3);
    Ok(Context {
        identity: id.slug.clone(),
        project: req.project.map(str::to_string),
        items,
        now: status::now_block(&state, req.project),
        kb,
        warnings,
    })
}

pub fn render_text(id: &Identity, c: &Context) -> String {
    let mut out = String::new();
    let mut now_done = false;
    for it in &c.items {
        if it.tier != "core" && !now_done {
            out.push_str(&format!("<!-- owo: now -->\n{}\n", c.now));
            now_done = true;
        }
        let text = fs::read_to_string(id.dir.join(&it.path)).unwrap_or_default();
        out.push_str(&format!(
            "<!-- owo: {} ({}) -->\n{}\n",
            it.path,
            it.tier,
            text.trim_end()
        ));
        out.push('\n');
    }
    if !now_done {
        out.push_str(&format!("<!-- owo: now -->\n{}\n", c.now));
    }
    if let Some(kb) = &c.kb {
        let note = if kb.exists {
            kb.entry.clone().unwrap_or_else(|| kb.path.clone())
        } else {
            format!("{} (not found)", kb.path)
        };
        out.push_str(&format!(
            "<!-- owo: project kb -->\nProject knowledge base: {note}\n"
        ));
    }
    out
}
