use crate::{Error, ErrorKind, Fault, Kind, Result, Rule, Scope, Spec, SyncFormat};
use chrono::NaiveDate;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const MAX_DAYS: i64 = 36500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Switch,
    Text,
    Int,
    Day,
    Path,
    Choice(Vec<&'static str>),
    Json,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: &'static str,
    pub help: &'static str,
    pub ty: Type,
    pub positional: bool,
    pub many: bool,
    pub required: bool,
    pub default: Option<String>,
    pub value_name: Option<&'static str>,
    pub hyphen: bool,
    pub range: Option<(i64, i64)>,
    pub conflicts: Option<&'static str>,
    pub requires: Option<&'static str>,
}

impl Field {
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

    pub fn switch(name: &'static str, help: &'static str) -> Self {
        Self::new(name, Type::Switch, help)
    }

    pub fn positional(name: &'static str, ty: Type, help: &'static str) -> Self {
        Self {
            positional: true,
            ..Self::new(name, ty, help)
        }
    }

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

#[derive(Debug, Clone, Default)]
pub struct Args {
    map: Map<String, Value>,
    paths: BTreeMap<String, PathBuf>,
}

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
    pub fn new(name: &str, fields: &[Field], map: Map<String, Value>) -> Result<Self> {
        if let Some(k) = map.keys().find(|k| !fields.iter().any(|f| f.name == *k)) {
            return Err(usage(format!("unknown argument {k:?} for {name}")));
        }
        let args = Args {
            map,
            paths: BTreeMap::new(),
        };
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
        }
        Ok(args)
    }

    pub fn with_path(mut self, name: &str, path: PathBuf) -> Self {
        self.paths.insert(name.to_string(), path);
        self
    }

    fn check_rules(&self, f: &Field) -> Result<()> {
        if !self.given(f.name) {
            if f.required && f.ty == Type::Path {
                return Err(usage(format!("missing argument {:?}", f.name)));
            }
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

    fn get(&self, name: &str) -> Option<&Value> {
        self.map.get(name).filter(|v| !v.is_null())
    }

    fn given(&self, name: &str) -> bool {
        !matches!(self.get(name), None | Some(Value::Bool(false)))
    }

    fn one(&self, name: &str) -> Option<&Value> {
        match self.get(name)? {
            Value::Array(items) if items.len() == 1 => Some(&items[0]),
            v => Some(v),
        }
    }

    fn strings(&self, name: &str) -> impl Iterator<Item = String> + '_ {
        let items = match self.get(name) {
            Some(Value::Array(items)) => items.as_slice(),
            Some(v) => std::slice::from_ref(v),
            None => &[],
        };
        items.iter().filter_map(scalar)
    }
}

enum Mode<'a> {
    Declare,
    Args(&'a Args),
    Check,
}

pub struct Reader<'a> {
    mode: Mode<'a>,
    fields: Vec<Field>,
    values: Map<String, Value>,
    error: Option<Error>,
}

pub trait Number: Sized {
    fn parse(text: &str) -> Option<Self>;
    fn value(&self) -> Value;
}

macro_rules! number {
    ($($t:ty),*) => {$(
        impl Number for $t {
            fn parse(text: &str) -> Option<Self> {
                text.parse().ok()
            }
            fn value(&self) -> Value {
                (*self).into()
            }
        }
    )*};
}

number!(usize, i64);

impl<N: Number> Number for Option<N> {
    fn parse(text: &str) -> Option<Self> {
        N::parse(text).map(Some)
    }
    fn value(&self) -> Value {
        self.as_ref().map_or(Value::Null, N::value)
    }
}

fn given(is: bool) -> Value {
    if is {
        Value::Bool(true)
    } else {
        Value::Null
    }
}

impl<'a> Reader<'a> {
    fn new(mode: Mode<'a>) -> Self {
        Self {
            mode,
            fields: vec![],
            values: Map::new(),
            error: None,
        }
    }

    fn declare(&mut self, f: Field, value: Value) -> Option<(&'static str, &'a Args)> {
        let name = f.name;
        self.fields.push(f);
        match self.mode {
            Mode::Declare => None,
            Mode::Check => {
                self.values.insert(name.into(), value);
                None
            }
            Mode::Args(args) => Some((name, args)),
        }
    }

    fn fail(&mut self, message: String) {
        self.error.get_or_insert_with(|| usage(message));
    }

    fn one(&mut self, name: &str, args: &Args) -> Option<String> {
        let text = scalar(args.one(name)?);
        if text.is_none() {
            let ty = match &self.fields.last().expect("a declared field").ty {
                Type::Int => "an integer".to_string(),
                Type::Day => "a YYYY-MM-DD date".to_string(),
                Type::Choice(values) => format!("one of {}", values.join(", ")),
                _ => "a string".to_string(),
            };
            self.fail(format!("argument {name:?} must be {ty}"));
        }
        text
    }

    pub fn flag(&mut self, f: Field, base: bool) -> bool {
        match self.declare(f, base.into()) {
            Some((name, a)) => a.get(name).and_then(Value::as_bool) == Some(true),
            None => base,
        }
    }

    pub fn text(&mut self, f: Field, base: Option<String>) -> Option<String> {
        match self.declare(f, given(base.is_some())) {
            Some((name, a)) => self.one(name, a).or(base),
            None => base,
        }
    }

    pub fn string(&mut self, f: Field, base: String) -> String {
        match self.declare(f, given(!base.is_empty())) {
            Some((name, a)) => self.one(name, a).unwrap_or(base),
            None => base,
        }
    }

    fn one_path(&mut self, name: &str, args: &Args) -> Option<PathBuf> {
        match args.paths.get(name) {
            Some(p) => Some(p.clone()),
            None => self.one(name, args).map(PathBuf::from),
        }
    }

    pub fn path(&mut self, f: Field, base: Option<PathBuf>) -> Option<PathBuf> {
        match self.declare(f, given(base.is_some())) {
            Some((name, a)) => self.one_path(name, a).or(base),
            None => base,
        }
    }

    pub fn required_path(&mut self, f: Field, base: PathBuf) -> PathBuf {
        match self.declare(f, given(!base.as_os_str().is_empty())) {
            Some((name, a)) => self.one_path(name, a).unwrap_or(base),
            None => base,
        }
    }

    pub fn list(&mut self, f: Field, base: Vec<String>) -> Vec<String> {
        match self.declare(f, given(!base.is_empty())) {
            Some((name, a)) if a.get(name).is_some() => a.strings(name).collect(),
            _ => base,
        }
    }

    pub fn words(&mut self, f: Field, base: String) -> String {
        match self.declare(f, given(!base.is_empty())) {
            Some((name, a)) if a.get(name).is_some() => {
                a.strings(name).collect::<Vec<_>>().join(" ")
            }
            _ => base,
        }
    }

    pub fn int<T: Number>(&mut self, f: Field, base: T) -> T {
        let Some((name, a)) = self.declare(f, base.value()) else {
            return base;
        };
        let Some(text) = self.one(name, a) else {
            return base;
        };
        T::parse(&text).unwrap_or_else(|| {
            self.fail(format!(
                "argument {name:?} must be an integer, not {text:?}"
            ));
            base
        })
    }

    pub fn day(&mut self, f: Field, base: Option<NaiveDate>) -> Option<NaiveDate> {
        let name = f.name;
        let Some((name, a)) = self.declare(f, given(base.is_some())) else {
            if let (Mode::Check, Some(day)) = (&self.mode, base) {
                if let Err(e) = crate::parse_day(&day.to_string()) {
                    self.fail(format!("argument {name:?}: {e}"));
                }
            }
            return base;
        };
        let Some(text) = self.one(name, a) else {
            return base;
        };
        match crate::parse_day(&text) {
            Ok(day) => Some(day),
            Err(e) => {
                self.fail(format!("argument {name:?}: {e}"));
                base
            }
        }
    }

    pub fn named<T>(
        &mut self,
        f: Field,
        from: fn(&str) -> Option<T>,
        base: Option<T>,
    ) -> Option<T> {
        match self.declare(f, given(base.is_some())) {
            Some((name, a)) => self.one(name, a).and_then(|s| from(&s)).or(base),
            None => base,
        }
    }

    pub fn choice<T>(&mut self, f: Field, from: fn(&str) -> Option<T>, base: T) -> T {
        match self.declare(f, given(true)) {
            Some((name, a)) => self.one(name, a).and_then(|s| from(&s)).unwrap_or(base),
            None => base,
        }
    }

    pub fn all_named<T>(&mut self, f: Field, from: fn(&str) -> Option<T>, base: Vec<T>) -> Vec<T> {
        match self.declare(f, given(!base.is_empty())) {
            Some((name, a)) if a.get(name).is_some() => {
                a.strings(name).filter_map(|s| from(&s)).collect()
            }
            _ => base,
        }
    }
}

pub trait Request: Default + Clone {
    fn read(r: &mut Reader, base: Self) -> Self;

    fn fields() -> Vec<Field> {
        let mut r = Reader::new(Mode::Declare);
        Self::read(&mut r, Self::default());
        r.fields
    }

    fn from_args(args: &Args) -> Result<Self> {
        let mut r = Reader::new(Mode::Args(args));
        let req = Self::read(&mut r, Self::default());
        r.error.map_or(Ok(req), Err)
    }

    fn check(&self) -> Result<()> {
        let mut r = Reader::new(Mode::Check);
        Self::read(&mut r, self.clone());
        if let Some(e) = r.error {
            return Err(e);
        }
        let values = Args {
            map: r.values,
            paths: BTreeMap::new(),
        };
        r.fields.iter().try_for_each(|f| values.check_rules(f))
    }
}

impl Request for () {
    fn read(_: &mut Reader, _: Self) -> Self {}
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DryRun<R> {
    pub req: R,
    pub dry_run: bool,
}

impl<R: Request> Request for DryRun<R> {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            req: R::read(r, b.req),
            dry_run: r.flag(
                Field::switch("dryRun", "Report the files it would write, and write none"),
                b.dry_run,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NewRequest {
    pub name: String,
    pub slug: Option<String>,
}

impl Request for NewRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            name: r.string(
                Field::positional("name", Type::Text, "Display name, for example \"Thea\"")
                    .required(),
                b.name,
            ),
            slug: r.text(
                Field::new(
                    "slug",
                    Type::Text,
                    "Slug override (default: slug of the name)",
                ),
                b.slug,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WriteRequest {
    pub path: String,
    pub content: Option<String>,
}

impl Request for WriteRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            path: r.string(
                Field::positional(
                    "path",
                    Type::Text,
                    "Identity-relative path under soul/, life/ or work/, ending in .md",
                )
                .required(),
                b.path,
            ),
            content: r.text(
                Field::new("content", Type::Text, "The file text").hyphen(),
                b.content,
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AddRequest {
    pub kind: Kind,
    pub title: String,
    pub date: Option<NaiveDate>,
    pub parent: Option<String>,
    pub aliases: Vec<String>,
    pub kb: Option<String>,
}

impl Default for AddRequest {
    fn default() -> Self {
        Self {
            kind: Kind::Log,
            title: String::new(),
            date: None,
            parent: None,
            aliases: vec![],
            kb: None,
        }
    }
}

impl Request for AddRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            kind: r.choice(
                Field::positional(
                    "kind",
                    Type::Choice(crate::entry_types().collect()),
                    "Entry type",
                )
                .required()
                .value_name("TYPE"),
                Kind::from_type,
                b.kind,
            ),
            title: r.words(
                Field::words("title", "Title (a log without one is titled by its date)"),
                b.title,
            ),
            date: r.day(
                Field::new(
                    "date",
                    Type::Day,
                    "Date for dated entries (default: today, local time)",
                ),
                b.date,
            ),
            parent: r.text(
                Field::new("parent", Type::Text, "Parent location for location-detail"),
                b.parent,
            ),
            aliases: r.list(
                Field::new("alias", Type::Text, "Alias (repeatable)")
                    .many()
                    .value_name("ALIASES"),
                b.aliases,
            ),
            kb: r.text(
                Field::new(
                    "kb",
                    Type::Text,
                    "KB path for a project, relative to the identity folder",
                ),
                b.kb,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StampRequest {
    pub check: bool,
}

impl Request for StampRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            check: r.flag(
                Field::switch("check", "Report drift instead of writing"),
                b.check,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LintRequest {
    pub rules: Vec<Rule>,
    pub changed: bool,
}

impl Request for LintRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
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
                b.rules,
            ),
            changed: r.flag(
                Field::switch(
                    "changed",
                    "Report only files changed since the last commit or stamp",
                ),
                b.changed,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IndexRequest {
    pub check: bool,
}

impl Request for IndexRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            check: r.flag(
                Field::switch("check", "Report stale generated files instead of writing"),
                b.check,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SlugRequest {
    pub text: String,
}

impl Request for SlugRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            text: r.words(Field::words("text", "Text to turn into a slug"), b.text),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TemplateRequest {
    pub name: String,
}

impl Request for TemplateRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            name: r.string(
                Field::positional("name", Type::Text, "Template name, as `templates` lists it")
                    .required(),
                b.name,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolveRequest {
    pub name: String,
    pub kind: Option<Kind>,
}

impl Request for ResolveRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            name: r.words(
                Field::words("name", "Entry name, alias, path or [[link]]").required(),
                b.name,
            ),
            kind: r.named(
                Field::new(
                    "type",
                    Type::Choice(Kind::names().collect()),
                    "Limit to one entry type",
                )
                .value_name("KIND"),
                Kind::from_name,
                b.kind,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShowRequest {
    pub name: String,
    pub section: Option<String>,
}

impl Request for ShowRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            name: r.words(
                Field::words(
                    "name",
                    "Entry name, alias, path, [[link]] or kb:<project>:<path>",
                ),
                b.name,
            ),
            section: r.text(
                Field::new(
                    "section",
                    Type::Text,
                    "Print only this section (a heading at any level)",
                ),
                b.section,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinksRequest {
    pub name: String,
    pub inbound: bool,
    pub outbound: bool,
    pub orphans: bool,
}

impl Request for LinksRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            name: r.words(
                Field::words(
                    "name",
                    "Entry name, alias, path or [[link]] (not needed with orphans)",
                ),
                b.name,
            ),
            inbound: r.flag(
                Field::switch("in", "Only links pointing at the entry"),
                b.inbound,
            ),
            outbound: r.flag(
                Field::switch("out", "Only links going out of the entry"),
                b.outbound,
            ),
            orphans: r.flag(
                Field::switch("orphans", "List entries nothing links to"),
                b.orphans,
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextRequest {
    pub voice: bool,
    pub look: bool,
    pub with: Vec<String>,
    pub notes: usize,
    pub paths: bool,
}

impl Default for ContextRequest {
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
    fn read(r: &mut Reader, b: Self) -> Self {
        let d = Self::default();
        Self {
            voice: r.flag(Field::switch("voice", "Add soul/VOICE.md"), b.voice),
            look: r.flag(Field::switch("look", "Add soul/LOOK.md"), b.look),
            with: r.list(
                Field::new("with", Type::Text, "Add a resolvable entry (repeatable)").many(),
                b.with,
            ),
            notes: r.int(
                Field::new("notes", Type::Int, "Recent project notes to include").default(d.notes),
                b.notes,
            ),
            paths: r.choice(
                Field::new(
                    "format",
                    Type::Choice(vec!["paths", "text"]),
                    "text: file contents concatenated; paths: the list",
                )
                .default("text"),
                |s| Some(s == "paths"),
                b.paths,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StateRequest {
    pub days: i64,
}

impl Default for StateRequest {
    fn default() -> Self {
        Self { days: 14 }
    }
}

impl Request for StateRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        let d = Self::default();
        Self {
            days: r.int(
                Field::new("days", Type::Int, "Window for recent events")
                    .default(d.days)
                    .range(0, MAX_DAYS),
                b.days,
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub scope: Option<Scope>,
    pub types: Vec<Kind>,
    pub project: Option<String>,
    pub since: Option<NaiveDate>,
    pub limit: usize,
}

impl Default for SearchRequest {
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
    fn read(r: &mut Reader, b: Self) -> Self {
        let d = Self::default();
        Self {
            query: r.words(Field::words("query", "Search words"), b.query),
            scope: r.named(
                Field::new(
                    "scope",
                    Type::Choice(Scope::ALL.map(Scope::name).to_vec()),
                    "Limit to soul, life or work",
                ),
                Scope::from_name,
                b.scope,
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
                b.types,
            ),
            project: b.project,
            since: r.day(
                Field::new(
                    "since",
                    Type::Day,
                    "Only entries dated (or updated) on or after YYYY-MM-DD",
                ),
                b.since,
            ),
            limit: r.int(
                Field::new("limit", Type::Int, "Most hits to print").default(d.limit),
                b.limit,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReviewDoneRequest {
    pub through: Option<NaiveDate>,
}

impl Request for ReviewDoneRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            through: r.day(
                Field::new(
                    "through",
                    Type::Day,
                    "Last story day covered (default: newest log, event or journal date)",
                ),
                b.through,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoulNoteRequest {
    pub file: String,
    pub change: String,
    pub evidence: Vec<String>,
}

impl Request for SoulNoteRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            file: r.string(
                Field::positional("file", Type::Text, "Soul file, for example SOUL.md").required(),
                b.file,
            ),
            change: r.string(
                Field::new("change", Type::Text, "What changed, in one line").required(),
                b.change,
            ),
            evidence: r.list(
                Field::new(
                    "evidence",
                    Type::Text,
                    "Evidence entry name or [[link]] (repeatable)",
                )
                .many(),
                b.evidence,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SoulRequest {
    pub identity: bool,
    pub soul: bool,
    pub voice: bool,
    pub look: bool,
    pub memory: bool,
}

impl Request for SoulRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            identity: r.flag(Field::switch("id", "Print soul/IDENTITY.md"), b.identity),
            soul: r.flag(Field::switch("soul", "Print soul/SOUL.md"), b.soul),
            voice: r.flag(Field::switch("voice", "Print soul/VOICE.md"), b.voice),
            look: r.flag(Field::switch("look", "Print soul/LOOK.md"), b.look),
            memory: r.flag(Field::switch("memory", "Print soul/MEMORY.md"), b.memory),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoulHistoryRequest {
    pub file: Option<String>,
}

impl Request for SoulHistoryRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            file: r.text(
                Field::positional(
                    "file",
                    Type::Text,
                    "Soul file, for example SOUL.md (default: every file)",
                ),
                b.file,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChangesRequest {
    pub since: Option<String>,
    pub days: Option<i64>,
}

impl Request for ChangesRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            since: r.text(
                Field::new(
                    "since",
                    Type::Text,
                    "YYYY-MM-DD or anything `git log --since` accepts",
                )
                .conflicts("days"),
                b.since,
            ),
            days: r.int(
                Field::new("days", Type::Int, "Only the last N days").range(0, MAX_DAYS),
                b.days,
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CardRequest {
    pub spec: Spec,
    pub project: Option<String>,
    pub scenario: Option<String>,
    pub out: Option<PathBuf>,
    pub png: Option<PathBuf>,
}

impl Default for CardRequest {
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
    fn read(r: &mut Reader, b: Self) -> Self {
        let d = Self::default();
        let png = r.path(
            Field::new(
                "png",
                Type::Path,
                "Embed the card in this PNG avatar (needs out)",
            )
            .requires("out"),
            b.png,
        );
        let scenario = r.text(
            Field::new(
                "scenario",
                Type::Text,
                "Event or location whose summary becomes the scenario",
            ),
            b.scenario,
        );
        let spec = r.choice(
            Field::new(
                "spec",
                Type::Choice(Spec::ALL.map(Spec::name).to_vec()),
                "Card spec version",
            )
            .default(d.spec.name()),
            Spec::from_name,
            b.spec,
        );
        let out = r.path(
            Field::new("out", Type::Path, "Write to a file instead of stdout"),
            b.out,
        );
        Self {
            spec,
            project: b.project,
            scenario,
            out,
            png,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SkillRequest {
    pub out: PathBuf,
}

impl Request for SkillRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            out: r.required_path(
                Field::new("out", Type::Path, "Folder to write SKILL.md into").required(),
                b.out,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SoulspecRequest {
    pub out: PathBuf,
}

impl Request for SoulspecRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            out: r.required_path(
                Field::new("out", Type::Path, "Package folder to write").required(),
                b.out,
            ),
        }
    }
}

pub const DEFAULT_SYNC_EVERY: usize = 6;

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    pub now: bool,
    pub every: Option<usize>,
}

impl Request for SyncOptions {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            now: r.flag(
                Field::switch("now", "Take any new turns now (session end)"),
                b.now,
            ),
            every: r.int(
                Field::new(
                    "every",
                    Type::Int,
                    "New user turns per run (default: sync_every in the config, else 6)",
                ),
                b.every,
            ),
        }
    }
}

fn session_field(help: &'static str) -> Field {
    Field::new("session", Type::Text, help)
        .required()
        .value_name("SESSION_ID")
}

const SESSION_HELP: &str = "Session id: the host's id for this chat (letters, digits, -, _ or .)";

#[derive(Debug, Clone, Default)]
pub struct SyncStartRequest {
    pub session: String,
    pub transcript: PathBuf,
    pub format: SyncFormat,
    pub options: SyncOptions,
}

impl Request for SyncStartRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            session: r.string(session_field(SESSION_HELP), b.session),
            transcript: r.required_path(
                Field::new(
                    "transcript",
                    Type::Path,
                    "Transcript file the host keeps for this session",
                )
                .required(),
                b.transcript,
            ),
            format: r.choice(
                Field::new(
                    "format",
                    Type::Choice(SyncFormat::ALL.map(SyncFormat::name).to_vec()),
                    "Transcript format; jsonl lines are {\"role\": ..., \"text\": ...} with an optional turn \"id\"",
                )
                .default(SyncFormat::default().name()),
                SyncFormat::from_name,
                b.format,
            ),
            options: SyncOptions::read(r, b.options),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncPushRequest {
    pub session: String,
    pub turns: Option<String>,
    pub options: SyncOptions,
}

impl Request for SyncPushRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            session: r.string(session_field(SESSION_HELP), b.session),
            turns: r.text(
                Field::new(
                    "turns",
                    Type::Json,
                    "JSON array of {\"id\", \"role\", \"text\"}; id is the turn id, the host's id for that message, and a stored turn id is skipped",
                )
                .hyphen(),
                b.turns,
            ),
            options: SyncOptions::read(r, b.options),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SyncDoneRequest {
    pub session: String,
    pub until: usize,
}

impl Request for SyncDoneRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            session: r.string(
                session_field("Session id given to sync start or sync push"),
                b.session,
            ),
            until: r.int(
                Field::new(
                    "until",
                    Type::Int,
                    "Message index the ingest run reached (the until from sync start)",
                )
                .required(),
                b.until,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MigrateRequest {
    pub path: PathBuf,
    pub out: PathBuf,
}

impl Request for MigrateRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            path: r.required_path(
                Field::positional("path", Type::Path, "The v1 identity folder").required(),
                b.path,
            ),
            out: r.required_path(
                Field::new("out", Type::Path, "New identity folder, <root>/<slug>").required(),
                b.out,
            ),
        }
    }
}
