//! The request type of each operation. A request type lists its fields with `Request::fields`:
//! the name, type, help text, default, allowed values and rules of each. The CLI builds its
//! flags from them and the MCP server its parameters, and both map their arguments into the
//! request with `Request::from_args`. The typed function of each operation checks the rules
//! when it takes its request.

use crate::{Fault, Kind, Result, Rule, Scope, Spec, SyncFormat};
use chrono::NaiveDate;
use serde_json::{Map, Value};
use std::path::PathBuf;

/// The largest number of days `state` and `changes` take.
pub const MAX_DAYS: i64 = 36500;

/// The type of a field's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// On when given; takes no value.
    Switch,
    Text,
    /// A whole number; one with a range is an `i64`, else a `usize`.
    Int,
    /// A `YYYY-MM-DD` day.
    Day,
    Path,
    /// One of these names.
    Choice(Vec<&'static str>),
    /// A JSON document, such as the turns of `sync push`.
    Json,
}

/// One field of a request: an argument of its operation.
#[derive(Debug, Clone)]
pub struct Field {
    /// The flag name (`--name`), the positional name, and the MCP parameter name.
    pub name: &'static str,
    pub help: &'static str,
    pub ty: Type,
    /// Given by place, not by flag.
    pub positional: bool,
    /// Takes several values: a repeatable flag, or the words of a positional.
    pub many: bool,
    pub required: bool,
    pub default: Option<String>,
    /// The name of the value in help text, when it is not the field name in capitals.
    pub value_name: Option<&'static str>,
    /// Takes a value that starts with `-`.
    pub hyphen: bool,
    /// The smallest and largest value of an `Int`.
    pub range: Option<(i64, i64)>,
    /// A field that cannot be given with this one.
    pub conflicts: Option<&'static str>,
    /// A field this one needs.
    pub requires: Option<&'static str>,
}

impl Field {
    /// A flag with one value of type `ty`.
    pub fn new(name: &'static str, ty: Type, help: &'static str) -> Self {
        Self {
            name,
            help,
            ty,
            positional: false,
            many: false,
            required: false,
            default: None,
            value_name: None,
            hyphen: false,
            range: None,
            conflicts: None,
            requires: None,
        }
    }

    /// A flag with no value.
    pub fn switch(name: &'static str, help: &'static str) -> Self {
        Self::new(name, Type::Switch, help)
    }

    /// A positional value of type `ty`.
    pub fn positional(name: &'static str, ty: Type, help: &'static str) -> Self {
        Self {
            positional: true,
            ..Self::new(name, ty, help)
        }
    }

    /// A positional value of several words, joined with spaces.
    pub fn words(name: &'static str, help: &'static str) -> Self {
        Self::positional(name, Type::Text, help).many()
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn many(mut self) -> Self {
        self.many = true;
        self
    }

    pub fn default(mut self, value: impl ToString) -> Self {
        self.default = Some(value.to_string());
        self
    }

    pub fn value_name(mut self, name: &'static str) -> Self {
        self.value_name = Some(name);
        self
    }

    pub fn hyphen(mut self) -> Self {
        self.hyphen = true;
        self
    }

    pub fn range(mut self, min: i64, max: i64) -> Self {
        self.range = Some((min, max));
        self
    }

    pub fn conflicts(mut self, name: &'static str) -> Self {
        self.conflicts = Some(name);
        self
    }

    pub fn requires(mut self, name: &'static str) -> Self {
        self.requires = Some(name);
        self
    }
}

/// The arguments of one call, by field name: a JSON value each.
#[derive(Debug, Clone, Default)]
pub struct Args {
    map: Map<String, Value>,
}

fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

impl Args {
    /// The arguments in `map` for the operation `name` with `fields`: each one a field, each
    /// required field there, and each value of a field with a fixed list in it.
    pub fn new(name: &str, fields: &[Field], map: Map<String, Value>) -> Result<Self, String> {
        if let Some(k) = map.keys().find(|k| !fields.iter().any(|f| f.name == *k)) {
            return Err(format!("unknown argument {k:?} for {name}"));
        }
        let args = Args { map };
        for f in fields {
            if f.required && args.get(f.name).is_none() {
                return Err(format!("missing argument {:?}", f.name));
            }
            let Type::Choice(values) = &f.ty else {
                continue;
            };
            for v in args.strings(f.name) {
                if !values.contains(&v.as_str()) {
                    return Err(format!(
                        "invalid value {v:?} for {:?}; possible values: {}",
                        f.name,
                        values.join(", ")
                    ));
                }
            }
        }
        Ok(args)
    }

    /// An argument that is not null.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.map.get(name).filter(|v| !v.is_null())
    }

    /// The scalar values of an argument: each item of an array, or the one value.
    pub fn strings(&self, name: &str) -> Vec<String> {
        match self.get(name) {
            Some(Value::Array(items)) => items.iter().filter_map(scalar).collect(),
            Some(v) => scalar(v).into_iter().collect(),
            None => vec![],
        }
    }

    /// A single-value argument: a scalar, or an array of one scalar.
    pub fn text(&self, name: &str) -> Result<Option<String>, String> {
        let one = match self.get(name) {
            None => return Ok(None),
            Some(Value::Array(items)) if items.len() == 1 => &items[0],
            Some(v) => v,
        };
        scalar(one)
            .map(Some)
            .ok_or_else(|| format!("argument {name:?} must be a string"))
    }

    /// A required single-value argument; `Args::new` checked it is there.
    pub fn required(&self, name: &str) -> Result<String, String> {
        Ok(self.text(name)?.unwrap_or_default())
    }

    /// A positional argument of several words: a string, or an array of words.
    pub fn words(&self, name: &str) -> String {
        self.strings(name).join(" ")
    }

    pub fn flag(&self, name: &str) -> bool {
        self.get(name).and_then(Value::as_bool) == Some(true)
    }

    /// An integer argument: a number, or a string of one.
    pub fn int<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, String> {
        self.text(name)?
            .map(|s| {
                s.parse()
                    .map_err(|_| format!("argument {name:?} must be an integer, not {s:?}"))
            })
            .transpose()
    }

    /// A `YYYY-MM-DD` argument.
    pub fn day(&self, name: &str) -> Result<Option<NaiveDate>, String> {
        self.text(name)?
            .map(|s| crate::parse_day(&s).map_err(|e| format!("argument {name:?}: {e}")))
            .transpose()
    }

    pub fn path(&self, name: &str) -> Result<Option<PathBuf>, String> {
        Ok(self.text(name)?.map(PathBuf::from))
    }

    /// A value of a fixed list, as its typed value. `Args::new` checked the name.
    pub fn named<T>(&self, name: &str, from: fn(&str) -> Option<T>) -> Result<Option<T>, String> {
        Ok(self.text(name)?.and_then(|s| from(&s)))
    }

    /// The values of a repeatable field with a fixed list, as their typed values.
    pub fn all_named<T>(&self, name: &str, from: fn(&str) -> Option<T>) -> Vec<T> {
        self.strings(name).iter().filter_map(|s| from(s)).collect()
    }
}

/// The arguments of one operation.
pub trait Request: Sized {
    /// The fields, in the order the CLI lists them.
    fn fields() -> Vec<Field>;
    /// The request from checked arguments, with the defaults for the arguments left out. The
    /// error is the text of an argument of the wrong type.
    fn from_args(args: &Args) -> Result<Self, String>;
}

/// An operation with no arguments.
impl Request for () {
    fn fields() -> Vec<Field> {
        vec![]
    }
    fn from_args(_: &Args) -> Result<Self, String> {
        Ok(())
    }
}

/// A number of days from 0 to `MAX_DAYS`.
pub(crate) fn check_days(days: i64) -> Result<i64> {
    if !(0..=MAX_DAYS).contains(&days) {
        return Err(Fault::OutOfRange {
            name: "days",
            value: days,
            min: 0,
            max: MAX_DAYS,
        }
        .into());
    }
    Ok(days)
}

#[derive(Debug, Clone, Default)]
pub struct NewRequest {
    /// The display name.
    pub name: String,
    /// The slug; the slug of the name when None.
    pub slug: Option<String>,
}

impl Request for NewRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::positional("name", Type::Text, "Display name, for example \"Thea\"").required(),
            Field::new(
                "slug",
                Type::Text,
                "Slug override (default: slug of the name)",
            ),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            name: a.required("name")?,
            slug: a.text("slug")?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct WriteRequest {
    /// The identity-relative path.
    pub path: String,
    /// The file text. The operation reads the caller's input when it is None; `Identity::write`
    /// writes an empty file.
    pub content: Option<String>,
}

impl Request for WriteRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::positional(
                "path",
                Type::Text,
                "Identity-relative path under soul/, life/ or work/, ending in .md",
            )
            .required(),
            Field::new(
                "content",
                Type::Text,
                "The file text; read from stdin when left out",
            )
            .hyphen(),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            path: a.required("path")?,
            content: a.text("content")?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AddRequest {
    pub kind: Kind,
    /// A log without one is titled by its date.
    pub title: String,
    pub date: Option<NaiveDate>,
    pub parent: Option<String>,
    pub aliases: Vec<String>,
    /// KB path for a project (`kb:` in project.md)
    pub kb: Option<String>,
}

impl Request for AddRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::positional(
                "kind",
                Type::Choice(crate::entry_types().collect()),
                "Entry type",
            )
            .required()
            .value_name("TYPE"),
            Field::words("title", "Title (a log without one is titled by its date)"),
            Field::new(
                "date",
                Type::Day,
                "Date for dated entries (default: today, local time)",
            ),
            Field::new("parent", Type::Text, "Parent location for location-detail"),
            Field::new("alias", Type::Text, "Alias (repeatable)")
                .many()
                .value_name("ALIASES"),
            Field::new(
                "kb",
                Type::Text,
                "KB path for a project, relative to the identity folder",
            ),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            kind: a
                .named("kind", Kind::from_type)?
                .ok_or("missing argument \"kind\"")?,
            title: a.words("title"),
            date: a.day("date")?,
            parent: a.text("parent")?,
            aliases: a.strings("alias"),
            kb: a.text("kb")?,
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StampRequest {
    /// Only report the drift.
    pub check: bool,
}

impl Request for StampRequest {
    fn fields() -> Vec<Field> {
        vec![Field::switch(
            "check",
            "Report drift and exit 1 instead of writing",
        )]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            check: a.flag("check"),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct LintRequest {
    /// Every rule when empty.
    pub rules: Vec<Rule>,
    /// Only the findings in files changed since the last commit or stamp.
    pub changed: bool,
}

impl Request for LintRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::new(
                "rule",
                Type::Choice(Rule::all().map(Rule::name).collect()),
                "Run only these rules (repeatable)",
            )
            .many()
            .value_name("RULES"),
            Field::switch(
                "changed",
                "Report only files changed since the last commit or stamp",
            ),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            rules: a.all_named("rule", Rule::from_name),
            changed: a.flag("changed"),
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IndexRequest {
    /// Only list the stale generated files.
    pub check: bool,
}

impl Request for IndexRequest {
    fn fields() -> Vec<Field> {
        vec![Field::switch(
            "check",
            "Report stale generated files and exit 1 instead of writing",
        )]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            check: a.flag("check"),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SlugRequest {
    pub text: String,
}

impl Request for SlugRequest {
    fn fields() -> Vec<Field> {
        vec![Field::words("text", "Text to turn into a slug")]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            text: a.words("text"),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TemplateRequest {
    pub name: String,
}

impl Request for TemplateRequest {
    fn fields() -> Vec<Field> {
        vec![Field::positional(
            "name",
            Type::Text,
            "Template name from `owo templates list`",
        )
        .required()]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            name: a.required("name")?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolveRequest {
    pub name: String,
    /// Only entries of this type.
    pub kind: Option<Kind>,
}

impl Request for ResolveRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::words("name", "Entry name, alias, path or [[link]]"),
            Field::new(
                "type",
                Type::Choice(Kind::names().collect()),
                "Limit to one entry type",
            )
            .value_name("KIND"),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            name: a.words("name"),
            kind: a.named("type", Kind::from_name)?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShowRequest {
    pub name: String,
    /// Only this section: a heading at any level.
    pub section: Option<String>,
}

impl Request for ShowRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::words(
                "name",
                "Entry name, alias, path, [[link]] or kb:<project>:<path>",
            ),
            Field::new(
                "section",
                Type::Text,
                "Print only this section (a heading at any level)",
            ),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            name: a.words("name"),
            section: a.text("section")?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinksRequest {
    pub name: String,
    /// Only the links into the entry. Both when neither is set.
    pub inbound: bool,
    /// Only the links out of the entry.
    pub outbound: bool,
    /// The entries nothing links to, instead of the links of `name`.
    pub orphans: bool,
}

impl Request for LinksRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::words(
                "name",
                "Entry name, alias, path or [[link]] (not needed with --orphans)",
            ),
            Field::switch("in", "Only links pointing at the entry"),
            Field::switch("out", "Only links going out of the entry"),
            Field::switch("orphans", "List entries nothing links to"),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            name: a.words("name"),
            inbound: a.flag("in"),
            outbound: a.flag("out"),
            orphans: a.flag("orphans"),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ContextRequest {
    pub voice: bool,
    pub look: bool,
    /// Resolvable entries to add.
    pub with: Vec<String>,
    /// Recent project notes to include.
    pub notes: usize,
    /// The file list instead of the file contents.
    pub paths: bool,
}

impl Default for ContextRequest {
    /// The core files and three recent project notes, as text.
    fn default() -> Self {
        Self {
            voice: false,
            look: false,
            with: vec![],
            notes: 3,
            paths: false,
        }
    }
}

impl Request for ContextRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::switch("voice", "Add soul/VOICE.md"),
            Field::switch("look", "Add soul/LOOK.md"),
            Field::new("with", Type::Text, "Add a resolvable entry (repeatable)").many(),
            Field::new("notes", Type::Int, "Recent project notes to include")
                .default(Self::default().notes),
            Field::new(
                "format",
                Type::Choice(vec!["paths", "text"]),
                "text: file contents concatenated; paths: the list",
            )
            .default("text"),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        let d = Self::default();
        Ok(Self {
            voice: a.flag("voice"),
            look: a.flag("look"),
            with: a.strings("with"),
            notes: a.int("notes")?.unwrap_or(d.notes),
            paths: a.text("format")?.as_deref() == Some("paths"),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StateRequest {
    /// The window for recent events, from 0 to `MAX_DAYS`.
    pub days: i64,
}

impl Default for StateRequest {
    /// Events of the last 14 days.
    fn default() -> Self {
        Self { days: 14 }
    }
}

impl Request for StateRequest {
    fn fields() -> Vec<Field> {
        vec![Field::new("days", Type::Int, "Window for recent events")
            .default(Self::default().days)
            .range(0, MAX_DAYS)]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            days: a.int("days")?.unwrap_or(Self::default().days),
        })
    }
}

/// Which entries a search looks at, and how many hits it returns.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub scope: Option<Scope>,
    /// Every kind when empty.
    pub types: Vec<Kind>,
    /// Only `work/projects/<p>/`. The operation sets it to the selected project.
    pub project: Option<String>,
    /// Story `date`, else `updated`, on or after this day.
    pub since: Option<NaiveDate>,
    /// The most hits to return.
    pub limit: usize,
}

impl Default for SearchRequest {
    /// Every entry, and ten hits.
    fn default() -> Self {
        Self {
            query: String::new(),
            scope: None,
            types: vec![],
            project: None,
            since: None,
            limit: 10,
        }
    }
}

impl Request for SearchRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::words("query", "Search words"),
            Field::new(
                "scope",
                Type::Choice(Scope::ALL.map(Scope::name).to_vec()),
                "Limit to soul, life or work",
            ),
            Field::new(
                "type",
                Type::Choice(Kind::names().collect()),
                "Limit to entry types (repeatable)",
            )
            .many()
            .value_name("TYPES"),
            Field::new(
                "since",
                Type::Day,
                "Only entries dated (or updated) on or after YYYY-MM-DD",
            ),
            Field::new("limit", Type::Int, "Most hits to print").default(Self::default().limit),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            query: a.words("query"),
            scope: a.named("scope", Scope::from_name)?,
            types: a.all_named("type", Kind::from_name),
            project: None,
            since: a.day("since")?,
            limit: a.int("limit")?.unwrap_or(Self::default().limit),
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReviewDoneRequest {
    /// The last story day covered; the newest one when None.
    pub through: Option<NaiveDate>,
}

impl Request for ReviewDoneRequest {
    fn fields() -> Vec<Field> {
        vec![Field::new(
            "through",
            Type::Day,
            "Last story day covered (default: newest log, event or journal date)",
        )]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            through: a.day("through")?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoulNoteRequest {
    /// The soul file, for example SOUL.md.
    pub file: String,
    pub change: String,
    /// Evidence entry names or links.
    pub evidence: Vec<String>,
}

impl Request for SoulNoteRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::positional("file", Type::Text, "Soul file, for example SOUL.md").required(),
            Field::new("change", Type::Text, "What changed, in one line").required(),
            Field::new(
                "evidence",
                Type::Text,
                "Evidence entry name or [[link]] (repeatable)",
            )
            .many(),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            file: a.required("file")?,
            change: a.required("change")?,
            evidence: a.strings("evidence"),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoulHistoryRequest {
    /// One soul file; every file when None.
    pub file: Option<String>,
}

impl Request for SoulHistoryRequest {
    fn fields() -> Vec<Field> {
        vec![Field::positional(
            "file",
            Type::Text,
            "Soul file, for example SOUL.md (default: every file)",
        )]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            file: a.text("file")?,
        })
    }
}

/// Which changes `changes` lists: all by default.
#[derive(Debug, Clone, Default)]
pub struct ChangesRequest {
    /// A day, or what `git log --since` takes.
    pub since: Option<String>,
    /// Only the last N days, from 0 to `MAX_DAYS`; not with `since`.
    pub days: Option<i64>,
}

impl Request for ChangesRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::new(
                "since",
                Type::Text,
                "YYYY-MM-DD or anything `git log --since` accepts",
            )
            .conflicts("days"),
            Field::new("days", Type::Int, "Only the last N days").range(0, MAX_DAYS),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            since: a.text("since")?,
            days: a.int("days")?,
        })
    }
}

/// What `export_card` builds, and where it writes the card.
#[derive(Debug, Clone)]
pub struct CardRequest {
    pub spec: Spec,
    /// The project whose notes join the card; it must exist. The operation sets it to the
    /// selected project.
    pub project: Option<String>,
    /// The event or location whose summary becomes the scenario.
    pub scenario: Option<String>,
    /// Writes the card to this file when set.
    pub out: Option<PathBuf>,
    /// Embeds the card in this PNG avatar; needs `out`.
    pub png: Option<PathBuf>,
}

impl Default for CardRequest {
    /// A V3 card of the whole identity, not written.
    fn default() -> Self {
        Self {
            spec: Spec::V3,
            project: None,
            scenario: None,
            out: None,
            png: None,
        }
    }
}

impl Request for CardRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::new(
                "png",
                Type::Path,
                "Embed the card in this PNG avatar (needs --out)",
            )
            .requires("out"),
            Field::new(
                "scenario",
                Type::Text,
                "Event or location whose summary becomes the scenario",
            ),
            Field::new(
                "spec",
                Type::Choice(Spec::ALL.map(Spec::name).to_vec()),
                "Card spec version",
            )
            .default(Self::default().spec.name()),
            Field::new("out", Type::Path, "Write to a file instead of stdout"),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            spec: a
                .named("spec", Spec::from_name)?
                .unwrap_or(Self::default().spec),
            project: None,
            scenario: a.text("scenario")?,
            out: a.path("out")?,
            png: a.path("png")?,
        })
    }
}

/// An export into a folder.
#[derive(Debug, Clone, Default)]
pub struct SkillRequest {
    /// The folder to write SKILL.md into.
    pub out: PathBuf,
}

impl Request for SkillRequest {
    fn fields() -> Vec<Field> {
        vec![Field::new("out", Type::Path, "Folder to write SKILL.md into").required()]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            out: a.required("out")?.into(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoulspecRequest {
    /// The package folder to write.
    pub out: PathBuf,
}

impl Request for SoulspecRequest {
    fn fields() -> Vec<Field> {
        vec![Field::new("out", Type::Path, "Package folder to write").required()]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            out: a.required("out")?.into(),
        })
    }
}

/// When a sync step writes a slice for ingest.
#[derive(Debug, Clone, Copy)]
pub struct SyncOptions {
    /// Take any new turns now (session end).
    pub now: bool,
    /// New user turns per run.
    pub every: usize,
}

impl Default for SyncOptions {
    /// A run every six new user turns.
    fn default() -> Self {
        Self {
            now: false,
            every: 6,
        }
    }
}

impl Request for SyncOptions {
    fn fields() -> Vec<Field> {
        vec![
            Field::switch("now", "Take any new turns now (session end)"),
            Field::new("every", Type::Int, "New user turns per run").default(Self::default().every),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            now: a.flag("now"),
            every: a.int("every")?.unwrap_or(Self::default().every),
        })
    }
}

fn session_field(help: &'static str) -> Field {
    Field::new("session", Type::Text, help)
        .required()
        .value_name("SESSION_ID")
}

const SESSION_HELP: &str = "Session id: the host's id for this chat (letters, digits, -, _ or .)";

#[derive(Debug, Clone)]
pub struct SyncStartRequest {
    pub session: String,
    /// The transcript file the host keeps for this session.
    pub transcript: PathBuf,
    pub format: SyncFormat,
    pub options: SyncOptions,
}

impl Request for SyncStartRequest {
    fn fields() -> Vec<Field> {
        let mut fields = vec![
            session_field(SESSION_HELP),
            Field::new(
                "transcript",
                Type::Path,
                "Transcript file the host keeps for this session",
            )
            .required(),
            Field::new(
                "format",
                Type::Choice(SyncFormat::ALL.map(SyncFormat::name).to_vec()),
                "Transcript format; jsonl lines are {\"role\": ..., \"text\": ...} with an optional turn \"id\"",
            )
            .default(SyncFormat::default().name()),
        ];
        fields.extend(SyncOptions::fields());
        fields
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            session: a.required("session")?,
            transcript: a.required("transcript")?.into(),
            format: a
                .named("format", SyncFormat::from_name)?
                .unwrap_or_default(),
            options: SyncOptions::from_args(a)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SyncPushRequest {
    pub session: String,
    /// The turns as a JSON array of `{id, role, text}`. The operation reads the caller's input
    /// when it is None, and parses it with `parse_turns`.
    pub turns: Option<String>,
    pub options: SyncOptions,
}

impl Request for SyncPushRequest {
    fn fields() -> Vec<Field> {
        let mut fields = vec![
            session_field(SESSION_HELP),
            Field::new(
                "turns",
                Type::Json,
                "JSON array of {\"id\", \"role\", \"text\"}; id is the turn id, the host's id for that message, and a stored turn id is skipped (default: stdin)",
            )
            .hyphen(),
        ];
        fields.extend(SyncOptions::fields());
        fields
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            session: a.required("session")?,
            turns: a.text("turns")?,
            options: SyncOptions::from_args(a)?,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncDoneRequest {
    pub session: String,
    /// The message index the ingest run reached.
    pub until: usize,
}

impl Request for SyncDoneRequest {
    fn fields() -> Vec<Field> {
        vec![
            session_field("Session id given to sync start or sync push"),
            Field::new(
                "until",
                Type::Int,
                "Message index the ingest run reached (the until from sync start)",
            )
            .required(),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            session: a.required("session")?,
            until: a.int("until")?.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct MigrateRequest {
    /// The v1 identity folder.
    pub path: PathBuf,
    /// The new identity folder, `<root>/<slug>`.
    pub out: PathBuf,
}

impl Request for MigrateRequest {
    fn fields() -> Vec<Field> {
        vec![
            Field::positional("path", Type::Path, "The v1 identity folder").required(),
            Field::new("out", Type::Path, "New identity folder, <root>/<slug>").required(),
        ]
    }
    fn from_args(a: &Args) -> Result<Self, String> {
        Ok(Self {
            path: a.required("path")?.into(),
            out: a.required("out")?.into(),
        })
    }
}
