//! `owo context`: the tiered load set for an identity.
//! Core (always): IDENTITY, SOUL, MEMORY and the generated "Now" block.
//! Voice: VOICE.md. Look: LOOK.md. Project: work README, project.md, recent notes, KB pointer.

use crate::identity::{Identity, Kind};
use crate::status;
use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Request<'a> {
    pub voice: bool,
    pub look: bool,
    pub project: Option<&'a str>,
    /// KB path from `.owo.toml` (already absolute), used when identity.json has none.
    pub config_kb: Option<PathBuf>,
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
    pub kb: Option<Kb>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Kb {
    pub path: String,
    pub exists: bool,
    pub entry: Option<String>,
}

fn item(tier: &'static str, path: &str, reason: &str) -> Item {
    Item {
        tier,
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

/// KB path for a project: project.md `kb`, else identity.json `projects.<p>.kb`
/// (both relative to the identity folder; `~/` is the home folder), else the `.owo.toml` value.
pub fn kb_path(id: &Identity, project: &str, config_kb: Option<&Path>) -> Option<PathBuf> {
    let from_note = id
        .entry(&format!("work/projects/{project}/project.md"))
        .and_then(|e| e.field_str("kb"))
        .filter(|s| !s.is_empty());
    let from_manifest = from_note.or_else(|| {
        id.manifest
            .as_ref()
            .and_then(|m| m.pointer(&format!("/projects/{project}/kb")))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    match from_manifest {
        Some(raw) => {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            let p = crate::config::expand_home(&raw, home.as_deref());
            Some(if p.is_absolute() { p } else { id.dir.join(p) })
        }
        None => config_kb.map(Path::to_path_buf),
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
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
        let prefix = format!("work/projects/{p}/notes/");
        let mut notes: Vec<_> = id
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Note && e.rel.starts_with(&prefix))
            .collect();
        notes.sort_by(|a, b| b.rel.cmp(&a.rel));
        for n in notes.into_iter().take(req.notes) {
            items.push(item("project", &n.rel, "recent project note"));
        }
        match kb_path(id, p, req.config_kb.as_deref()) {
            Some(path) => {
                let path = normalize(&path);
                let exists = path.exists();
                let entry = if path.is_file() {
                    Some(path.display().to_string())
                } else {
                    ["lat.md", "README.md", "index.md"]
                        .iter()
                        .map(|f| path.join(f))
                        .find(|f| f.is_file())
                        .map(|f| f.display().to_string())
                };
                if !exists {
                    warnings.push(format!("project KB not found at {}", path.display()));
                }
                kb = Some(Kb {
                    path: path.display().to_string(),
                    exists,
                    entry,
                });
            }
            None => warnings.push(format!(
                "project {p} has no KB path (identity.json projects.{p}.kb or .owo.toml kb)"
            )),
        }
    }
    for name in req.with {
        match crate::find::resolve(id, name, None) {
            crate::find::Resolution::Found(i) => items.push(item(
                "with",
                &id.entries[i].rel,
                &format!("requested: {name}"),
            )),
            _ => warnings.push(format!("--with {name:?} does not resolve")),
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

/// Records a project session in `work/state.json`.
pub fn touch_project(id: &Identity, project: &str) -> Result<()> {
    let path = id.dir.join("work/state.json");
    let mut doc = id
        .read_json("work/state.json")
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({ "projects": {}, "files": {} }));
    let at = crate::state::now();
    if !doc.get("projects").is_some_and(|v| v.is_object()) {
        doc["projects"] = json!({});
    }
    let entry = &mut doc["projects"][project];
    if entry.get("first_touched").is_none() {
        entry["first_touched"] = json!(at);
    }
    entry["last_touched"] = json!(at);
    entry["sessions"] = json!(entry.get("sessions").and_then(|v| v.as_u64()).unwrap_or(0) + 1);
    crate::identity::write_json(&path, &doc)
}

pub fn render_paths(c: &Context) -> String {
    let mut out = String::new();
    for it in &c.items {
        out.push_str(&format!("{:<8} {}  ({})\n", it.tier, it.path, it.reason));
    }
    out.push_str("now      (generated block)\n");
    if let Some(kb) = &c.kb {
        out.push_str(&format!(
            "kb       {}{}\n",
            kb.path,
            if kb.exists { "" } else { "  (missing)" }
        ));
    }
    out
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
