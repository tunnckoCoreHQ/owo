use crate::identity::Identity;
use crate::status;
use anyhow::Result;
use serde::Serialize;
use std::fs;

use crate::ContextRequest as Request;

named! {
    #[derive(Default)]
    pub enum Format {
        Paths = "paths",
        #[default]
        Text = "text",
    }
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
    pub current_state: String,
    pub wiki: Option<crate::wiki::WikiPath>,
    pub warnings: Vec<String>,
}

impl Context {
    pub fn paths_text(&self) -> String {
        let items = self
            .items
            .iter()
            .map(|it| format!("{:<8} {}  ({})\n", it.tier, it.path, it.reason));
        let wiki = self.wiki.iter().map(|wiki| {
            let missing = if wiki.exists { "" } else { "  (missing)" };
            format!("wiki     {}{missing}\n", wiki.path)
        });
        items
            .chain(wiki)
            .chain(["state    (generated current state)\n".to_string()])
            .collect()
    }
}

fn item(tier: &'static str, path: &str, reason: &str) -> Item {
    Item {
        tier,
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

pub fn build(id: &Identity, req: &Request, project: Option<&str>) -> Result<Context> {
    let mut items = vec![
        item("core", "persona/IDENTITY.md", "who the identity is"),
        item("core", "persona/SOUL.md", "first-person core"),
        item("core", "persona/MEMORY.md", "compiled long-term memory"),
    ];
    if req.voice {
        items.push(item(
            "voice",
            "persona/VOICE.md",
            "talking or writing as itself",
        ));
    }
    if req.look {
        items.push(item("look", "persona/LOOK.md", "visuals matter"));
    }
    let mut warnings = vec![];
    let mut wiki = None;
    if let Some(p) = project {
        let readme = format!("work/projects/{p}/README.md");
        items.push(item("work", "work/PROFILE.md", "work profile"));
        items.push(item("work", &readme, "role on this project"));
        for n in id.project_notes(p).into_iter().take(req.notes) {
            items.push(item("work", &n.rel, "recent project note"));
        }
        let (path, warning) = crate::wiki::path(id, p);
        wiki = path;
        warnings.extend(warning);
    }
    for name in &req.with {
        match crate::find::one(id, name) {
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
        project: project.map(str::to_string),
        items,
        current_state: status::current_state_text(&state, project),
        wiki,
        warnings,
    })
}

pub fn render_text(id: &Identity, c: &Context) -> String {
    let mut out = String::new();
    for it in &c.items {
        let text = fs::read_to_string(id.dir.join(&it.path)).unwrap_or_default();
        out.push_str(&format!(
            "<!-- owo: {} ({}) -->\n{}\n\n",
            it.path,
            it.tier,
            text.trim_end()
        ));
    }
    if let Some(wiki) = &c.wiki {
        let shown = if wiki.exists {
            wiki.entry.clone().unwrap_or_else(|| wiki.path.clone())
        } else {
            format!("{} (not found)", wiki.path)
        };
        out.push_str(&format!(
            "<!-- owo: project wiki -->\nProject wiki: {shown}\n\n"
        ));
    }
    out.push_str(&format!(
        "<!-- owo: current state -->\n{}\n",
        c.current_state
    ));
    out
}
