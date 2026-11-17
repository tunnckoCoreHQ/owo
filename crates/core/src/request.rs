use crate::{
    CardSpec, ContextFormat, Error, ErrorKind, Fault, Guide, Kind, Result, Rule, Scope, SyncFormat,
};
use chrono::NaiveDate;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

pub const MAX_DAYS: i64 = 36500;

pub trait Named: Copy + PartialEq + 'static {
    fn rows() -> impl Iterator<Item = (Self, &'static str)>;

    fn name(self) -> &'static str {
        Self::rows()
            .find(|r| r.0 == self)
            .expect("a listed value")
            .1
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::rows().find(|r| r.1 == name).map(|r| r.0)
    }
}

macro_rules! named {
    ($(#[$m:meta])* pub enum $t:ident { $($(#[$a:meta])* $v:ident = $n:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        $(#[$m])*
        pub enum $t {
            $($(#[$a])* $v),+
        }

        impl $crate::Named for $t {
            fn rows() -> impl Iterator<Item = (Self, &'static str)> {
                [$(($t::$v, $n)),+].into_iter()
            }
        }
    };
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Type {
    Switch,
    #[default]
    Text,
    Int,
    Day,
    Path,
    Choice(Vec<&'static str>),
    Json,
}

#[derive(Debug, Clone, Default)]
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
    pub fn new(name: &'static str, help: &'static str) -> Self {
        Self {
            name,
            help,
            ..Default::default()
        }
    }

    pub fn positional(name: &'static str, help: &'static str) -> Self {
        Self {
            positional: true,
            ..Self::new(name, help)
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
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

    fn declare(&mut self, f: Field, ty: Type, value: Value) -> Option<(&'static str, &'a Args)> {
        let name = f.name;
        self.fields.push(Field { ty, ..f });
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
        match self.declare(f, Type::Switch, base.into()) {
            Some((name, a)) => a.get(name).and_then(Value::as_bool) == Some(true),
            None => base,
        }
    }

    pub fn text(&mut self, f: Field, base: Option<String>) -> Option<String> {
        match self.declare(f, Type::Text, given(base.is_some())) {
            Some((name, a)) => self.one(name, a).or(base),
            None => base,
        }
    }

    pub fn json(&mut self, f: Field, base: Option<String>) -> Option<String> {
        match self.declare(f, Type::Json, given(base.is_some())) {
            Some((name, a)) => self.one(name, a).or(base),
            None => base,
        }
    }

    pub fn string(&mut self, f: Field, base: String) -> String {
        let base = Some(base).filter(|b| !b.is_empty());
        self.text(f, base).unwrap_or_default()
    }

    fn one_path(&mut self, name: &str, args: &Args) -> Option<PathBuf> {
        match args.paths.get(name) {
            Some(p) => Some(p.clone()),
            None => self.one(name, args).map(PathBuf::from),
        }
    }

    pub fn path(&mut self, f: Field, base: Option<PathBuf>) -> Option<PathBuf> {
        match self.declare(f, Type::Path, given(base.is_some())) {
            Some((name, a)) => self.one_path(name, a).or(base),
            None => base,
        }
    }

    pub fn required_path(&mut self, f: Field, base: PathBuf) -> PathBuf {
        let base = Some(base).filter(|b| !b.as_os_str().is_empty());
        self.path(f.required(), base).unwrap_or_default()
    }

    pub fn list(&mut self, f: Field, base: Vec<String>) -> Vec<String> {
        let f = Field { many: true, ..f };
        match self.declare(f, Type::Text, given(!base.is_empty())) {
            Some((name, a)) if a.get(name).is_some() => a.strings(name).collect(),
            _ => base,
        }
    }

    pub fn words(&mut self, f: Field, base: String) -> String {
        let f = Field {
            positional: true,
            ..f
        };
        let base = Some(base).filter(|b| !b.is_empty()).into_iter().collect();
        self.list(f, base).join(" ")
    }

    pub fn int<T: FromStr + Into<Value> + Copy>(&mut self, f: Field, base: Option<T>) -> Option<T> {
        let Some((name, a)) = self.declare(f, Type::Int, base.into()) else {
            return base;
        };
        let Some(text) = self.one(name, a) else {
            return base;
        };
        text.parse().map(Some).unwrap_or_else(|_| {
            self.fail(format!(
                "argument {name:?} must be an integer, not {text:?}"
            ));
            base
        })
    }

    pub fn int_or<T: FromStr + Into<Value> + Copy>(&mut self, f: Field, base: T) -> T {
        self.int(f, Some(base)).unwrap_or(base)
    }

    pub fn day(&mut self, f: Field, base: Option<NaiveDate>) -> Option<NaiveDate> {
        let name = f.name;
        let Some((name, a)) = self.declare(f, Type::Day, given(base.is_some())) else {
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

    pub fn choice_in<T: Named>(
        &mut self,
        f: Field,
        values: impl IntoIterator<Item = T>,
        base: T,
    ) -> T {
        let ty = Type::Choice(values.into_iter().map(T::name).collect());
        match self.declare(f, ty, given(true)) {
            Some((name, a)) => self
                .one(name, a)
                .and_then(|s| T::from_name(&s))
                .unwrap_or(base),
            None => base,
        }
    }

    pub fn named<T: Named>(&mut self, f: Field, base: Option<T>) -> Option<T> {
        let ty = Type::Choice(T::rows().map(|r| r.1).collect());
        match self.declare(f, ty, given(base.is_some())) {
            Some((name, a)) => self.one(name, a).and_then(|s| T::from_name(&s)).or(base),
            None => base,
        }
    }

    pub fn choice<T: Named>(&mut self, f: Field, base: T) -> T {
        self.named(f, Some(base)).unwrap_or(base)
    }

    pub fn all_named<T: Named>(&mut self, f: Field, base: Vec<T>) -> Vec<T> {
        let ty = Type::Choice(T::rows().map(|r| r.1).collect());
        match self.declare(Field { many: true, ..f }, ty, given(!base.is_empty())) {
            Some((name, a)) if a.get(name).is_some() => {
                a.strings(name).filter_map(|s| T::from_name(&s)).collect()
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
                Field::new("dryRun", "Report the files it would write, and write none"),
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
                Field::positional("name", "Display name, for example \"Thea\"").required(),
                b.name,
            ),
            slug: r.text(
                Field::new("slug", "Slug override (default: slug of the name)"),
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
                    "Identity-relative path under persona/, life/ or work/, ending in .md",
                )
                .required(),
                b.path,
            ),
            content: r.text(Field::new("content", "The file text").hyphen(), b.content),
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
    pub wiki: Option<String>,
}

impl Default for AddRequest {
    fn default() -> Self {
        Self {
            kind: Kind::DailyLog,
            title: String::new(),
            date: None,
            parent: None,
            aliases: vec![],
            wiki: None,
        }
    }
}

impl Request for AddRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            kind: r.choice_in(
                Field::positional("type", "Entry type").required(),
                crate::entry_types(),
                b.kind,
            ),
            title: r.words(
                Field::new(
                    "title",
                    "Title (a daily log needs one only to make the day's log)",
                ),
                b.title,
            ),
            date: r.day(
                Field::new(
                    "date",
                    "Date for dated entries (default: today, local time)",
                ),
                b.date,
            ),
            parent: r.text(
                Field::new("parent", "Parent location for location-detail"),
                b.parent,
            ),
            aliases: r.list(
                Field::new("alias", "Alias (repeatable)").value_name("ALIASES"),
                b.aliases,
            ),
            wiki: r.text(
                Field::new(
                    "wiki",
                    "Project wiki path of a project readme, relative to the identity folder",
                ),
                b.wiki,
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
                Field::new("check", "Report drift instead of writing"),
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
                Field::new("rule", "Run only these rules (repeatable)").value_name("RULES"),
                b.rules,
            ),
            changed: r.flag(
                Field::new(
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
                Field::new("check", "Report stale generated files instead of writing"),
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
            text: r.words(Field::new("text", "Text to turn into a slug"), b.text),
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
                Field::positional("name", "Template name, as `templates` lists it").required(),
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
                Field::new("name", "Entry name, alias, path or [[wikilink]]").required(),
                b.name,
            ),
            kind: r.named(Field::new("kind", "Limit to one kind"), b.kind),
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
                Field::new(
                    "name",
                    "Entry name, alias, path, [[wikilink]] or wiki:<project>:<path>",
                ),
                b.name,
            ),
            section: r.text(
                Field::new(
                    "section",
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
                Field::new(
                    "name",
                    "Entry name, alias, path or [[wikilink]] (not needed with orphans)",
                ),
                b.name,
            ),
            inbound: r.flag(
                Field::new("in", "Only wikilinks pointing at the entry"),
                b.inbound,
            ),
            outbound: r.flag(
                Field::new("out", "Only wikilinks going out of the entry"),
                b.outbound,
            ),
            orphans: r.flag(
                Field::new("orphans", "List entries nothing links to"),
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
    pub format: ContextFormat,
}

impl Default for ContextRequest {
    fn default() -> Self {
        Self {
            voice: false,
            look: false,
            with: vec![],
            notes: 3,
            format: ContextFormat::Text,
        }
    }
}

impl Request for ContextRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        let d = Self::default();
        Self {
            voice: r.flag(Field::new("voice", "Add persona/VOICE.md"), b.voice),
            look: r.flag(Field::new("look", "Add persona/LOOK.md"), b.look),
            with: r.list(
                Field::new("with", "Add a resolvable entry (repeatable)"),
                b.with,
            ),
            notes: r.int_or(
                Field::new("notes", "Recent project notes to include").default(d.notes),
                b.notes,
            ),
            format: r.choice(
                Field::new(
                    "format",
                    "text: file contents concatenated; paths: the list",
                )
                .default("text"),
                b.format,
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
            days: r.int_or(
                Field::new("days", "Window for recent events")
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
            query: r.words(Field::new("query", "Search words"), b.query),
            scope: r.named(
                Field::new("scope", "Limit to persona, life or work"),
                b.scope,
            ),
            types: r.all_named(
                Field::new("kind", "Limit to kinds (repeatable)").value_name("KINDS"),
                b.types,
            ),
            project: b.project,
            since: r.day(
                Field::new(
                    "since",
                    "Only entries dated (or updated) on or after YYYY-MM-DD",
                ),
                b.since,
            ),
            limit: r.int_or(
                Field::new("limit", "Most hits to print").default(d.limit),
                b.limit,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReflectDoneRequest {
    pub through: Option<NaiveDate>,
}

impl Request for ReflectDoneRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            through: r.day(
                Field::new(
                    "through",
                    "Last story day covered (default: newest daily log, event or journal date)",
                ),
                b.through,
            ),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PersonaChangeRequest {
    pub file: String,
    pub change: String,
    pub evidence: Vec<String>,
}

impl Request for PersonaChangeRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            file: r.string(
                Field::positional("file", "Persona file, for example SOUL.md").required(),
                b.file,
            ),
            change: r.string(
                Field::new("change", "What changed, in one line").required(),
                b.change,
            ),
            evidence: r.list(
                Field::new(
                    "evidence",
                    "Evidence entry name or [[wikilink]] (repeatable)",
                ),
                b.evidence,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PersonaRequest {
    pub identity: bool,
    pub soul: bool,
    pub voice: bool,
    pub look: bool,
    pub memory: bool,
}

impl Request for PersonaRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            identity: r.flag(Field::new("id", "Print persona/IDENTITY.md"), b.identity),
            soul: r.flag(Field::new("soul", "Print persona/SOUL.md"), b.soul),
            voice: r.flag(Field::new("voice", "Print persona/VOICE.md"), b.voice),
            look: r.flag(Field::new("look", "Print persona/LOOK.md"), b.look),
            memory: r.flag(Field::new("memory", "Print persona/MEMORY.md"), b.memory),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PersonaChangelogRequest {
    pub file: Option<String>,
}

impl Request for PersonaChangelogRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            file: r.text(
                Field::positional(
                    "file",
                    "Persona file, for example SOUL.md (default: every file)",
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
                Field::new("since", "YYYY-MM-DD or anything `git log --since` accepts")
                    .conflicts("days"),
                b.since,
            ),
            days: r.int(
                Field::new("days", "Only the last N days").range(0, MAX_DAYS),
                b.days,
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PersonaCardRequest {
    pub spec: CardSpec,
    pub project: Option<String>,
    pub scenario: Option<String>,
    pub out: Option<PathBuf>,
    pub png: Option<PathBuf>,
}

impl Default for PersonaCardRequest {
    fn default() -> Self {
        Self {
            spec: CardSpec::V3,
            project: None,
            scenario: None,
            out: None,
            png: None,
        }
    }
}

impl Request for PersonaCardRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        let d = Self::default();
        let png = r.path(
            Field::new("png", "Embed the card in this PNG avatar (needs out)").requires("out"),
            b.png,
        );
        let scenario = r.text(
            Field::new(
                "scenario",
                "Event or location whose summary becomes the scenario",
            ),
            b.scenario,
        );
        let spec = r.choice(
            Field::new("spec", "Character Card spec version").default(d.spec.name()),
            b.spec,
        );
        let out = r.path(
            Field::new("out", "Write to a file instead of stdout"),
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
            out: r.required_path(Field::new("out", "Folder to write SKILL.md into"), b.out),
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
            out: r.required_path(Field::new("out", "Package folder to write"), b.out),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GuideRequest {
    pub path: Option<Guide>,
}

impl Request for GuideRequest {
    fn read(r: &mut Reader, b: Self) -> Self {
        Self {
            path: r.named(
                Field::positional("path", "One file of the skill (default: its entry file)"),
                b.path,
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
                Field::new("now", "Take any new turns now (session end)"),
                b.now,
            ),
            every: r.int(
                Field::new(
                    "every",
                    "New user turns per run (default: sync_every in the config, else 6)",
                ),
                b.every,
            ),
        }
    }
}

fn session_field(help: &'static str) -> Field {
    Field::new("session", help)
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
                    "Transcript file the host keeps for this session",
                ),
                b.transcript,
            ),
            format: r.choice(
                Field::new(
                    "format",
                    "Transcript format; jsonl lines are {\"role\": ..., \"text\": ...} with an optional turn \"id\"",
                )
                .default(SyncFormat::default().name()),
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
            turns: r.json(
                Field::new(
                    "turns",
                    "JSON array of {\"id\", \"role\", \"text\"}; id is the turn id, the host's own id for that turn, and a stored turn id is skipped",
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
            until: r.int_or(
                Field::new(
                    "until",
                    "Turn index the ingest run reached (the until from sync start)",
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
            path: r.required_path(Field::positional("path", "The v1 identity folder"), b.path),
            out: r.required_path(
                Field::new("out", "New identity folder, <identities-dir>/<slug>"),
                b.out,
            ),
        }
    }
}
