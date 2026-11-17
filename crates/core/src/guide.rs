macro_rules! skill {
    ($f:expr) => {
        include_str!(concat!("../../../skills/owo/", $f))
    };
}

named! {
    pub enum Guide {
        Embody = "embody",
        Create = "create",
        Ingest = "ingest",
        Reflect = "reflect",
        Rules = "rules",
        Glossary = "glossary",
    }
}

/// The text of the owo skill: its entry file, or one of the files it routes to.
pub fn text(path: Option<Guide>) -> &'static str {
    match path {
        None => skill!("SKILL.md"),
        Some(Guide::Embody) => skill!("references/embody.md"),
        Some(Guide::Create) => skill!("references/create.md"),
        Some(Guide::Ingest) => skill!("references/ingest.md"),
        Some(Guide::Reflect) => skill!("references/reflect.md"),
        Some(Guide::Rules) => skill!("references/rules.md"),
        Some(Guide::Glossary) => include_str!("../../../GLOSSARY.md"),
    }
}
