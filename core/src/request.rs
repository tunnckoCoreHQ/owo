//! The request type of each operation. A request type reads itself with `Request::read`, and
//! each read declares one field: the name, type, help text, default, allowed values and rules.
//! `Request::fields` lists the fields: the CLI builds its flags from them and the MCP server its
//! parameters. `Args::new` checks arguments against the fields and their rules, and
//! `Request::from_args` maps them into the request.

use crate::{Error, ErrorKind, Fault, Kind, Result, Rule, Scope, Spec, SyncFormat};
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

/// The text of a string, number or boolean.
pub(crate) fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn usage(message: String) -> Error {
    Error::new(ErrorKind::Usage, message)
}

impl Args {
    /// The arguments in `map` for the operation `name` with `fields`, checked: each one a field,
    /// each required field there, each value of a field with a fixed list in it, and the range,
    /// conflicts and requires rules of each field.
    pub fn new(name: &str, fields: &[Field], map: Map<String, Value>) -> Result<Self> {
        if let Some(k) = map.keys().find(|k| !fields.iter().any(|f| f.name == *k)) {
            return Err(usage(format!("unknown argument {k:?} for {name}")));
        }
        let args = Args { map };
        for f in fields {
            if f.required && args.get(f.name).is_none() {
                return Err(usage(format!("missing argument {:?}", f.name)));
            }
            if let Type::Choice(values) = &f.ty {
                if let Some(v) = args.strings(f.name).find(|v| !values.contains(&v.as_str())) {
                    return Err(usage(format!(
                        "invalid value {v:?} for {:?}; possible values: {}",
                        f.name,
                        values.join(", ")
                    )));
                }
            }
            args.check_rules(f)?;
        }
        Ok(args)
    }

    /// Arguments that the caller checked against the fields and their rules itself, as the CLI
    /// does with its parser, which it builds from the same fields.
    pub fn checked(map: Map<String, Value>) -> Self {
        Args { map }
    }

    /// The range, conflicts and requires rules of `f`, when it is given.
    fn check_rules(&self, f: &Field) -> Result<()> {
        if !self.given(f.name) {
            return Ok(());
        }
        if let Some((min, max)) = f.range {
            let value = self
                .one(f.name)
                .and_then(scalar)
                .and_then(|s| s.parse().ok());
            if let Some(value) = value.filter(|v| !(min..=max).contains(v)) {
                return Err(Fault::OutOfRange {
                    name: f.name,
                    value,
                    min,
                    max,
                }
                .into());
            }
        }
        if let Some(other) = f.conflicts.filter(|o| self.given(o)) {
            return Err(Fault::Conflict(f.name, other).into());
        }
        if let Some(other) = f.requires.filter(|o| !self.given(o)) {
            return Err(Fault::Needs(f.name, other).into());
        }
        Ok(())
    }

    /// An argument that is not null.
    fn get(&self, name: &str) -> Option<&Value> {
        self.map.get(name).filter(|v| !v.is_null())
    }

    /// An argument that is there and not a switch that is off.
    fn given(&self, name: &str) -> bool {
        !matches!(self.get(name), None | Some(Value::Bool(false)))
    }

    /// A single-value argument: the value, or the item of an array of one.
    fn one(&self, name: &str) -> Option<&Value> {
        match self.get(name)? {
            Value::Array(items) if items.len() == 1 => Some(&items[0]),
            v => Some(v),
        }
    }

    /// The scalar values of an argument: each item of an array, or the one value.
    fn strings(&self, name: &str) -> impl Iterator<Item = String> + '_ {
        let items = match self.get(name) {
            Some(Value::Array(items)) => items.as_slice(),
            Some(v) => std::slice::from_ref(v),
            None => &[],
        };
        items.iter().filter_map(scalar)
    }
}

/// Reads a request from its arguments. Each read declares one field and gives its value;
/// without arguments it only collects the fields. A value of the wrong type is the first error.
pub struct Reader<'a> {
    args: Option<&'a Args>,
    fields: Vec<Field>,
    error: Option<Error>,
}

impl Reader<'_> {
    /// Declares `f` and returns its name.
    fn declare(&mut self, f: Field) -> &'static str {
        let name = f.name;
        self.fields.push(f);
        name
    }

    fn fail(&mut self, message: String) {
        self.error.get_or_insert_with(|| usage(message));
    }

    /// A switch.
    pub fn flag(&mut self, f: Field) -> bool {
        let name = self.declare(f);
        self.args.and_then(|a| a.get(name)).and_then(Value::as_bool) == Some(true)
    }

    /// A single-value text.
    pub fn text(&mut self, f: Field) -> Option<String> {
        let name = self.declare(f);
        let one = self.args.and_then(|a| a.one(name)).cloned()?;
        let text = scalar(&one);
        if text.is_none() {
            self.fail(format!("argument {name:?} must be a string"));
        }
        text
    }

    /// A required single-value text; `Args::new` checked it is there.
    pub fn string(&mut self, f: Field) -> String {
        self.text(f).unwrap_or_default()
    }

    /// Every value of a repeatable field.
    pub fn list(&mut self, f: Field) -> Vec<String> {
        let name = self.declare(f);
        self.args
            .map(|a| a.strings(name).collect())
            .unwrap_or_default()
    }

    /// The words of a positional, joined with spaces: a string, or an array of words.
    pub fn words(&mut self, f: Field) -> String {
        self.list(f).join(" ")
    }

    /// A value parsed from its text; the parse error follows the argument name.
    fn parsed<T>(
        &mut self,
        f: Field,
        parse: impl FnOnce(&str) -> std::result::Result<T, String>,
    ) -> Option<T> {
        let name = f.name;
        let text = self.text(f)?;
        match parse(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                self.fail(format!("argument {name:?}{e}"));
                None
            }
        }
    }

    /// An integer: a number, or a string of one.
    pub fn int<T: std::str::FromStr>(&mut self, f: Field) -> Option<T> {
        self.parsed(f, |s| {
            s.parse()
                .map_err(|_| format!(" must be an integer, not {s:?}"))
        })
    }

    /// A `YYYY-MM-DD` day.
    pub fn day(&mut self, f: Field) -> Option<NaiveDate> {
        self.parsed(f, |s| crate::parse_day(s).map_err(|e| format!(": {e}")))
    }

    pub fn path(&mut self, f: Field) -> Option<PathBuf> {
        self.text(f).map(PathBuf::from)
    }

    /// A value of a fixed list, as its typed value. `Args::new` checked the name.
    pub fn named<T>(&mut self, f: Field, from: fn(&str) -> Option<T>) -> Option<T> {
        self.text(f).and_then(|s| from(&s))
    }

    /// The values of a repeatable field with a fixed list, as their typed values.
    pub fn all_named<T>(&mut self, f: Field, from: fn(&str) -> Option<T>) -> Vec<T> {
        self.list(f).iter().filter_map(|s| from(s)).collect()
    }
}

/// The arguments of one operation.
pub trait Request: Sized {
    /// The request from `r`, with the defaults for the arguments left out. Each read declares
    /// one field, in the order the CLI lists them.
    fn read(r: &mut Reader) -> Self;

    /// The fields, in the order the CLI lists them.
    fn fields() -> Vec<Field> {
        let mut r = Reader {
            args: None,
            fields: vec![],
            error: None,
        };
        Self::read(&mut r);
        r.fields
    }

    /// The request from arguments checked with `Args::new`. The error is a usage error for an
    /// argument of the wrong type.
    fn from_args(args: &Args) -> Result<Self> {
        let mut r = Reader {
            args: Some(args),
            fields: vec![],
            error: None,
        };
        let req = Self::read(&mut r);
        r.error.map_or(Ok(req), Err)
    }
}

/// An operation with no arguments.
impl Request for () {
    fn read(_: &mut Reader) -> Self {}
}

#[derive(Debug, Clone, Default)]
pub struct NewRequest {
    /// The display name.
    pub name: String,
    /// The slug; the slug of the name when None.
    pub slug: Option<String>,
}

impl Request for NewRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            name: r.string(
                Field::positional("name", Type::Text, "Display name, for example \"Thea\"")
                    .required(),
            ),
            slug: r.text(Field::new(
                "slug",
                Type::Text,
                "Slug override (default: slug of the name)",
            )),
        }
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
    fn read(r: &mut Reader) -> Self {
        Self {
            path: r.string(
                Field::positional(
                    "path",
                    Type::Text,
                    "Identity-relative path under soul/, life/ or work/, ending in .md",
                )
                .required(),
            ),
            content: r.text(
                Field::new(
                    "content",
                    Type::Text,
                    "The file text; the caller's input when left out",
                )
                .hyphen(),
            ),
        }
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
    fn read(r: &mut Reader) -> Self {
        Self {
            kind: r
                .named(
                    Field::positional(
                        "kind",
                        Type::Choice(crate::entry_types().collect()),
                        "Entry type",
                    )
                    .required()
                    .value_name("TYPE"),
                    Kind::from_type,
                )
                // `Args::new` checked it is there; only the read that lists the fields lacks it.
                .unwrap_or(Kind::Log),
            title: r.words(Field::words(
                "title",
                "Title (a log without one is titled by its date)",
            )),
            date: r.day(Field::new(
                "date",
                Type::Day,
                "Date for dated entries (default: today, local time)",
            )),
            parent: r.text(Field::new(
                "parent",
                Type::Text,
                "Parent location for location-detail",
            )),
            aliases: r.list(
                Field::new("alias", Type::Text, "Alias (repeatable)")
                    .many()
                    .value_name("ALIASES"),
            ),
            kb: r.text(Field::new(
                "kb",
                Type::Text,
                "KB path for a project, relative to the identity folder",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StampRequest {
    /// Only report the drift.
    pub check: bool,
}

impl Request for StampRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            check: r.flag(Field::switch("check", "Report drift instead of writing")),
        }
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
    fn read(r: &mut Reader) -> Self {
        Self {
            rules: r.all_named(
                Field::new(
                    "rule",
                    Type::Choice(Rule::all().map(Rule::name).collect()),
                    "Run only these rules (repeatable)",
                )
                .many()
                .value_name("RULES"),
                Rule::from_name,
            ),
            changed: r.flag(Field::switch(
                "changed",
                "Report only files changed since the last commit or stamp",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IndexRequest {
    /// Only list the stale generated files.
    pub check: bool,
}

impl Request for IndexRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            check: r.flag(Field::switch(
                "check",
                "Report stale generated files instead of writing",
            )),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SlugRequest {
    pub text: String,
}

impl Request for SlugRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            text: r.words(Field::words("text", "Text to turn into a slug")),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TemplateRequest {
    pub name: String,
}

impl Request for TemplateRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            name: r.string(
                Field::positional("name", Type::Text, "Template name, as `templates` lists it")
                    .required(),
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolveRequest {
    pub name: String,
    /// Only entries of this type.
    pub kind: Option<Kind>,
}

impl Request for ResolveRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            name: r.words(Field::words("name", "Entry name, alias, path or [[link]]")),
            kind: r.named(
                Field::new(
                    "type",
                    Type::Choice(Kind::names().collect()),
                    "Limit to one entry type",
                )
                .value_name("KIND"),
                Kind::from_name,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShowRequest {
    pub name: String,
    /// Only this section: a heading at any level.
    pub section: Option<String>,
}

impl Request for ShowRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            name: r.words(Field::words(
                "name",
                "Entry name, alias, path, [[link]] or kb:<project>:<path>",
            )),
            section: r.text(Field::new(
                "section",
                Type::Text,
                "Print only this section (a heading at any level)",
            )),
        }
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
    fn read(r: &mut Reader) -> Self {
        Self {
            name: r.words(Field::words(
                "name",
                "Entry name, alias, path or [[link]] (not needed with orphans)",
            )),
            inbound: r.flag(Field::switch("in", "Only links pointing at the entry")),
            outbound: r.flag(Field::switch("out", "Only links going out of the entry")),
            orphans: r.flag(Field::switch("orphans", "List entries nothing links to")),
        }
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
    fn read(r: &mut Reader) -> Self {
        let d = Self::default();
        Self {
            voice: r.flag(Field::switch("voice", "Add soul/VOICE.md")),
            look: r.flag(Field::switch("look", "Add soul/LOOK.md")),
            with: r
                .list(Field::new("with", Type::Text, "Add a resolvable entry (repeatable)").many()),
            notes: r
                .int(
                    Field::new("notes", Type::Int, "Recent project notes to include")
                        .default(d.notes),
                )
                .unwrap_or(d.notes),
            paths: r
                .text(
                    Field::new(
                        "format",
                        Type::Choice(vec!["paths", "text"]),
                        "text: file contents concatenated; paths: the list",
                    )
                    .default("text"),
                )
                .as_deref()
                == Some("paths"),
        }
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
    fn read(r: &mut Reader) -> Self {
        let d = Self::default();
        Self {
            days: r
                .int(
                    Field::new("days", Type::Int, "Window for recent events")
                        .default(d.days)
                        .range(0, MAX_DAYS),
                )
                .unwrap_or(d.days),
        }
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
    fn read(r: &mut Reader) -> Self {
        let d = Self::default();
        Self {
            query: r.words(Field::words("query", "Search words")),
            scope: r.named(
                Field::new(
                    "scope",
                    Type::Choice(Scope::ALL.map(Scope::name).to_vec()),
                    "Limit to soul, life or work",
                ),
                Scope::from_name,
            ),
            types: r.all_named(
                Field::new(
                    "type",
                    Type::Choice(Kind::names().collect()),
                    "Limit to entry types (repeatable)",
                )
                .many()
                .value_name("TYPES"),
                Kind::from_name,
            ),
            project: None,
            since: r.day(Field::new(
                "since",
                Type::Day,
                "Only entries dated (or updated) on or after YYYY-MM-DD",
            )),
            limit: r
                .int(Field::new("limit", Type::Int, "Most hits to print").default(d.limit))
                .unwrap_or(d.limit),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReviewDoneRequest {
    /// The last story day covered; the newest one when None.
    pub through: Option<NaiveDate>,
}

impl Request for ReviewDoneRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            through: r.day(Field::new(
                "through",
                Type::Day,
                "Last story day covered (default: newest log, event or journal date)",
            )),
        }
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
    fn read(r: &mut Reader) -> Self {
        Self {
            file: r.string(
                Field::positional("file", Type::Text, "Soul file, for example SOUL.md").required(),
            ),
            change: r
                .string(Field::new("change", Type::Text, "What changed, in one line").required()),
            evidence: r.list(
                Field::new(
                    "evidence",
                    Type::Text,
                    "Evidence entry name or [[link]] (repeatable)",
                )
                .many(),
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoulHistoryRequest {
    /// One soul file; every file when None.
    pub file: Option<String>,
}

impl Request for SoulHistoryRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            file: r.text(Field::positional(
                "file",
                Type::Text,
                "Soul file, for example SOUL.md (default: every file)",
            )),
        }
    }
}

/// Which changes `changes` lists: all by default.
#[derive(Debug, Clone, Default)]
pub struct ChangesRequest {
    /// A day, or what `git log --since` takes. It wins over `days`, which its field rules
    /// forbid with it.
    pub since: Option<String>,
    /// Only the last N days, from 0 to `MAX_DAYS`.
    pub days: Option<i64>,
}

impl Request for ChangesRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            since: r.text(
                Field::new(
                    "since",
                    Type::Text,
                    "YYYY-MM-DD or anything `git log --since` accepts",
                )
                .conflicts("days"),
            ),
            days: r.int(Field::new("days", Type::Int, "Only the last N days").range(0, MAX_DAYS)),
        }
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
    /// Embeds the card in this PNG avatar; only with `out`, as its field rules say.
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
    fn read(r: &mut Reader) -> Self {
        let d = Self::default();
        let png = r.path(
            Field::new(
                "png",
                Type::Path,
                "Embed the card in this PNG avatar (needs out)",
            )
            .requires("out"),
        );
        let scenario = r.text(Field::new(
            "scenario",
            Type::Text,
            "Event or location whose summary becomes the scenario",
        ));
        let spec = r
            .named(
                Field::new(
                    "spec",
                    Type::Choice(Spec::ALL.map(Spec::name).to_vec()),
                    "Card spec version",
                )
                .default(d.spec.name()),
                Spec::from_name,
            )
            .unwrap_or(d.spec);
        let out = r.path(Field::new(
            "out",
            Type::Path,
            "Write to a file instead of stdout",
        ));
        Self {
            spec,
            project: None,
            scenario,
            out,
            png,
        }
    }
}

/// An export into a folder.
#[derive(Debug, Clone, Default)]
pub struct SkillRequest {
    /// The folder to write SKILL.md into.
    pub out: PathBuf,
}

impl Request for SkillRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            out: r
                .string(Field::new("out", Type::Path, "Folder to write SKILL.md into").required())
                .into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoulspecRequest {
    /// The package folder to write.
    pub out: PathBuf,
}

impl Request for SoulspecRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            out: r
                .string(Field::new("out", Type::Path, "Package folder to write").required())
                .into(),
        }
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
    fn read(r: &mut Reader) -> Self {
        let d = Self::default();
        Self {
            now: r.flag(Field::switch("now", "Take any new turns now (session end)")),
            every: r
                .int(Field::new("every", Type::Int, "New user turns per run").default(d.every))
                .unwrap_or(d.every),
        }
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
    fn read(r: &mut Reader) -> Self {
        Self {
            session: r.string(session_field(SESSION_HELP)),
            transcript: r
                .string(
                    Field::new(
                        "transcript",
                        Type::Path,
                        "Transcript file the host keeps for this session",
                    )
                    .required(),
                )
                .into(),
            format: r
                .named(
                    Field::new(
                        "format",
                        Type::Choice(SyncFormat::ALL.map(SyncFormat::name).to_vec()),
                        "Transcript format; jsonl lines are {\"role\": ..., \"text\": ...} with an optional turn \"id\"",
                    )
                    .default(SyncFormat::default().name()),
                    SyncFormat::from_name,
                )
                .unwrap_or_default(),
            options: SyncOptions::read(r),
        }
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
    fn read(r: &mut Reader) -> Self {
        Self {
            session: r.string(session_field(SESSION_HELP)),
            turns: r.text(
                Field::new(
                    "turns",
                    Type::Json,
                    "JSON array of {\"id\", \"role\", \"text\"}; id is the turn id, the host's id for that message, and a stored turn id is skipped (default: the caller's input)",
                )
                .hyphen(),
            ),
            options: SyncOptions::read(r),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncDoneRequest {
    pub session: String,
    /// The message index the ingest run reached.
    pub until: usize,
}

impl Request for SyncDoneRequest {
    fn read(r: &mut Reader) -> Self {
        Self {
            session: r.string(session_field("Session id given to sync start or sync push")),
            until: r
                .int(
                    Field::new(
                        "until",
                        Type::Int,
                        "Message index the ingest run reached (the until from sync start)",
                    )
                    .required(),
                )
                .unwrap_or_default(),
        }
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
    fn read(r: &mut Reader) -> Self {
        Self {
            path: r
                .string(Field::positional("path", Type::Path, "The v1 identity folder").required())
                .into(),
            out: r
                .string(
                    Field::new("out", Type::Path, "New identity folder, <root>/<slug>").required(),
                )
                .into(),
        }
    }
}
