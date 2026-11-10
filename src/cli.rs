//! The command line: every command, flag and help line. MCP tools are built from it.

use crate::identity::Kind;
use crate::{find, lint, state, sync, templates};
use chrono::NaiveDate;
use clap::builder::TypedValueParser;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "owo",
    version,
    about = "One world, one identity: manage living agent identities"
)]
pub struct Cli {
    /// Identities root (else OWO_ROOT, .owo.toml, user config, XDG default)
    #[arg(long, global = true)]
    pub root: Option<PathBuf>,
    /// Identity slug (else OWO_IDENTITY, .owo.toml, user config, the only identity)
    #[arg(short = 'i', long, global = true)]
    pub identity: Option<String>,
    /// Project slug (else OWO_PROJECT, .owo.toml); search and export card filter only on this flag
    #[arg(long, global = true)]
    pub project: Option<String>,
    /// Print the JSON envelope instead of text
    #[arg(long, global = true)]
    pub json: bool,
    /// Print only problems
    #[arg(short, long, global = true)]
    pub quiet: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new identity from the built-in templates
    New {
        /// Display name, for example "Thea"
        name: String,
        /// Slug override (default: slug of the name)
        #[arg(long)]
        slug: Option<String>,
    },
    /// List identities in the root
    List,
    /// Write the full text of one identity file (create or replace)
    Write {
        /// Identity-relative path under soul/, life/ or work/, ending in .md
        path: String,
        /// The file text; read from stdin when left out
        #[arg(long, allow_hyphen_values = true)]
        content: Option<String>,
    },
    /// Create an entry from its template at its deterministic path
    Add {
        /// Entry type
        #[arg(value_name = "TYPE", value_parser = clap::builder::PossibleValuesParser::new(templates::ENTRY_TEMPLATES.iter().map(|(n, _)| *n)).map(|s| Kind::from_type(&s).expect("an entry type")))]
        kind: Kind,
        /// Title (a log without one is titled by its date)
        title: Vec<String>,
        /// Date for dated entries (default: today, local time)
        #[arg(long, value_parser = state::parse_day)]
        date: Option<NaiveDate>,
        /// Parent location for location-detail
        #[arg(long)]
        parent: Option<String>,
        /// Alias (repeatable)
        #[arg(long = "alias")]
        aliases: Vec<String>,
        /// KB path for a project, relative to the identity folder
        #[arg(long)]
        kb: Option<String>,
    },
    /// Refresh state.json files from content hashes
    Stamp {
        /// Report drift and exit 1 instead of writing
        #[arg(long)]
        check: bool,
    },
    /// Validate an identity
    Lint {
        /// Run only these rules (repeatable)
        #[arg(long = "rule", value_parser = clap::builder::PossibleValuesParser::new(lint::rules()))]
        rules: Vec<String>,
        /// Report only files changed since the last commit or stamp
        #[arg(long)]
        changed: bool,
    },
    /// Regenerate _index.md files and life/timeline.md
    Index {
        /// Report stale generated files and exit 1 instead of writing
        #[arg(long)]
        check: bool,
    },
    /// Print the slug for a text
    Slug {
        /// Text to turn into a slug
        text: Vec<String>,
    },
    /// List templates, or show one (built in, or overridden in <root>/.templates/)
    Templates {
        #[command(subcommand)]
        action: Option<TemplatesAction>,
    },
    /// Resolve a name to an entry path the way links do
    Resolve {
        /// Entry name, alias, path or [[link]]
        name: Vec<String>,
        /// Limit to one entry type
        #[arg(long = "type", value_parser = clap::builder::PossibleValuesParser::new(find::types()))]
        kind: Option<String>,
    },
    /// Print an entry, or one section of it
    Show {
        /// Entry name, alias, path, [[link]] or kb:<project>:<path>
        name: Vec<String>,
        /// Print only this section (a heading at any level)
        #[arg(long)]
        section: Option<String>,
    },
    /// Backlinks and outlinks of an entry, or every orphan
    Links {
        /// Entry name, alias, path or [[link]] (not needed with --orphans)
        name: Vec<String>,
        /// Only links pointing at the entry
        #[arg(long = "in")]
        inbound: bool,
        /// Only links going out of the entry
        #[arg(long = "out")]
        outbound: bool,
        /// List entries nothing links to
        #[arg(long)]
        orphans: bool,
    },
    /// The tiered load set: core files, the generated Now block, and requested tiers
    Context {
        /// Add soul/VOICE.md
        #[arg(long)]
        voice: bool,
        /// Add soul/LOOK.md
        #[arg(long)]
        look: bool,
        /// Add a resolvable entry (repeatable)
        #[arg(long = "with")]
        with: Vec<String>,
        /// Recent project notes to include
        #[arg(long, default_value_t = 3)]
        notes: usize,
        /// paths: the list; text: file contents concatenated
        #[arg(long, default_value = "paths", value_parser = ["paths", "text"])]
        format: String,
    },
    /// Derived current state: activity, recent days and events, open threads, review
    State {
        /// Window for recent events
        #[arg(long, default_value_t = 14, value_parser = clap::value_parser!(i64).range(0..=36500))]
        days: i64,
    },
    /// Files updated in the last N days
    Recent {
        /// Window in days
        #[arg(long, default_value_t = 7, value_parser = clap::value_parser!(i64).range(0..=36500))]
        days: i64,
    },
    /// BM25 search over titles, aliases, summaries and bodies
    Search {
        /// Search words
        query: Vec<String>,
        /// Limit to soul, life or work
        #[arg(long, visible_alias = "in", value_parser = ["soul", "life", "work"])]
        scope: Option<String>,
        /// Limit to entry types (repeatable)
        #[arg(long = "type", value_parser = clap::builder::PossibleValuesParser::new(find::types()))]
        types: Vec<String>,
        /// Only entries dated (or updated) on or after YYYY-MM-DD
        #[arg(long, value_parser = state::parse_day)]
        since: Option<NaiveDate>,
        /// Most hits to print
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Print the review packet, or record a finished review with `done`
    Review {
        #[command(subcommand)]
        action: Option<ReviewAction>,
    },
    /// Soul history: evidence-backed records of soul changes
    Soul {
        #[command(subcommand)]
        action: SoulAction,
    },
    /// What changed, grouped by day (Git history, else state stamps)
    Changes {
        /// YYYY-MM-DD or anything `git log --since` accepts
        #[arg(long)]
        since: Option<String>,
    },
    /// Export the identity to another format
    Export {
        #[command(subcommand)]
        format: ExportFormat,
    },
    /// Session sync: slice a host transcript into sources/ for ingest
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },
    /// Migrate an identity from an older layout
    Migrate {
        #[command(subcommand)]
        from: MigrateFrom,
    },
    /// Show how root, identity and project resolve
    Config,
    /// Serve every command as an MCP tool, over stdio or HTTP with --remote
    Mcp {
        /// Serve over HTTP instead of stdio, on ADDR (default 127.0.0.1); prints the URL
        #[arg(long, value_name = "ADDR", num_args = 0..=1, default_missing_value = "127.0.0.1")]
        remote: Option<String>,
        /// Port for --remote (default 4224, or the next free one)
        #[arg(long, requires = "remote")]
        port: Option<u16>,
    },
}

#[derive(Subcommand)]
pub enum TemplatesAction {
    /// List template names and where each comes from
    List,
    /// Print a template as `owo new` and `owo add` would use it
    Show {
        /// Template name from `owo templates list`
        name: String,
    },
}
#[derive(Subcommand)]
pub enum ReviewAction {
    /// Record a finished review (dates and the MEMORY.md hash)
    Done {
        /// Last story day covered (default: newest log, event or journal date)
        #[arg(long, value_parser = state::parse_day)]
        through: Option<NaiveDate>,
    },
}

#[derive(Subcommand)]
pub enum SoulAction {
    /// Append a soul change with its evidence
    Note {
        /// Soul file, for example SOUL.md
        file: String,
        /// What changed, in one line
        #[arg(long)]
        change: String,
        /// Evidence entry name or [[link]] (repeatable)
        #[arg(long)]
        evidence: Vec<String>,
    },
    /// Print the soul history, optionally for one file
    History {
        /// Soul file, for example SOUL.md (default: every file)
        file: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ExportFormat {
    /// Character Card JSON (V3 with V2-compatible fields, or V2)
    Card {
        /// Embed the card in this PNG avatar (needs --out)
        #[arg(long, requires = "out")]
        png: Option<PathBuf>,
        /// Event or location whose summary becomes the scenario
        #[arg(long)]
        scenario: Option<String>,
        /// Card spec version
        #[arg(long, default_value = "v3", value_parser = ["v2", "v3"])]
        spec: String,
        /// Write to a file instead of stdout
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// SKILL.md wrapper that loads the identity with owo
    Skill {
        /// Folder to write SKILL.md into
        #[arg(long)]
        out: PathBuf,
    },
    /// SoulSpec v0.5 package folder
    Soulspec {
        /// Package folder to write
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum SyncAction {
    /// Read the transcript and, when a run is due, write the slice and the ingest prompt
    Start {
        /// Session id: the host's id for this chat (letters, digits, -, _ or .)
        #[arg(long, value_name = "SESSION_ID")]
        session: String,
        /// Transcript file the host keeps for this session
        #[arg(long)]
        transcript: PathBuf,
        /// Transcript format; jsonl lines are {"role": ..., "text": ...} with an optional turn "id"
        #[arg(long, default_value = "jsonl", value_parser = clap::builder::PossibleValuesParser::new(sync::FORMATS))]
        format: String,
        /// Take any new turns now (session end)
        #[arg(long)]
        now: bool,
        /// New user turns per run (default: identity.json sync_every, else 6)
        #[arg(long)]
        every: Option<usize>,
    },
    /// Store turns sent by the host, then run sync start on them (for hosts without a local transcript)
    Push {
        /// Session id: the host's id for this chat (letters, digits, -, _ or .)
        #[arg(long, value_name = "SESSION_ID")]
        session: String,
        /// JSON array of {"id", "role", "text"}; id is the turn id, the host's id for that message, and a stored turn id is skipped (default: stdin)
        #[arg(long, allow_hyphen_values = true)]
        turns: Option<String>,
        /// Take any new turns now (session end)
        #[arg(long)]
        now: bool,
        /// New user turns per run (default: identity.json sync_every, else 6)
        #[arg(long)]
        every: Option<usize>,
    },
    /// Move the bookmark after a successful ingest run
    Done {
        /// Session id given to sync start or sync push
        #[arg(long, value_name = "SESSION_ID")]
        session: String,
        /// Message index the ingest run reached (the until from sync start)
        #[arg(long)]
        until: usize,
    },
}

#[derive(Subcommand)]
pub enum MigrateFrom {
    /// v1 identity folder (with references/) into <out> = <root>/<slug>
    V1 {
        /// The v1 identity folder
        path: PathBuf,
        #[arg(long)]
        /// New identity folder, <root>/<slug>
        out: PathBuf,
    },
}
