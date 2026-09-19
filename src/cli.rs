//! Command-line interface definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Door 2 MeriaZard: browse local and remote data sources through one CLI.
#[derive(Debug, Parser)]
#[command(name = "d2mz", version, about, arg_required_else_help = true)]
pub struct Cli {
    /// Configuration file to use instead of the default.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List entries under a URI (defaults to the current directory).
    Ls(LsArgs),
    /// Print an object to standard output.
    Cat(CatArgs),
    /// Show metadata for an object.
    Stat(StatArgs),
    /// Search for objects below a URI.
    Find(FindArgs),
    /// Copy an object into the local archive.
    Ingest(IngestArgs),
    /// Re-check registered entries and update their presence state.
    Scan(ScanArgs),
    /// Retire entries so they are no longer tracked (a tombstone).
    Forget(ForgetArgs),
    /// Merge this index with a remote main database.
    Sync(SyncArgs),
    /// Open an object with an application chosen by kind.
    Open(OpenArgs),
    /// Manage application handlers used by `open`.
    Handler(HandlerArgs),
    /// Show or locate the cached thumbnail for a media object.
    Thumb(ThumbArgs),
    /// List archived entries.
    Archive(ArchiveArgs),
    /// Write an archived blob back out to a URI.
    Export(ExportArgs),
    /// Add, remove or list tags on archived entries.
    Tag(TagArgs),
    /// Full-text search across archived entries.
    Search(SearchArgs),
    /// Print a shell completion script to stdout.
    Completions(CompletionsArgs),
    /// Print the manual page (roff) to stdout.
    Man,
}

/// Arguments for `d2mz completions`.
#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

/// A byte-size filter value such as `10K`, `5M`, or `2G`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSize(pub u64);

impl std::str::FromStr for ByteSize {
    type Err = String;

    fn from_str(raw: &str) -> std::result::Result<ByteSize, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("empty size".to_string());
        }
        let (digits, unit) =
            raw.split_at(raw.find(|c: char| !c.is_ascii_digit()).unwrap_or(raw.len()));
        let value: u64 = digits
            .parse()
            .map_err(|_| format!("invalid number in {raw:?}"))?;
        let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
            "" | "b" => 1,
            "k" | "kb" | "kib" => 1024,
            "m" | "mb" | "mib" => 1024 * 1024,
            "g" | "gb" | "gib" => 1024 * 1024 * 1024,
            "t" | "tb" | "tib" => 1024_u64.pow(4),
            other => return Err(format!("unknown size unit {other:?}")),
        };
        value
            .checked_mul(multiplier)
            .map(ByteSize)
            .ok_or_else(|| format!("size {raw:?} overflows"))
    }
}

/// Arguments for `d2mz ls`.
#[derive(Debug, Args)]
pub struct LsArgs {
    /// Target URI.
    pub uri: Option<String>,

    /// Show a long listing.
    #[arg(short, long)]
    pub long: bool,

    /// List every entry below the target.
    #[arg(short = 'R', long)]
    pub recursive: bool,

    /// Emit the listing as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz cat`.
#[derive(Debug, Args)]
pub struct CatArgs {
    /// Target URI.
    pub uri: String,
}

/// Arguments for `d2mz stat`.
#[derive(Debug, Args)]
pub struct StatArgs {
    /// Target URI.
    pub uri: String,

    /// Emit the metadata as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz find`.
#[derive(Debug, Args)]
pub struct FindArgs {
    /// Base URI to search under.
    pub uri: Option<String>,

    /// Name glob, e.g. `*.log`. Comma-separated patterns are OR-ed.
    #[arg(long)]
    pub name: Option<String>,

    /// Minimum size, e.g. `10K`.
    #[arg(long, value_name = "SIZE")]
    pub min_size: Option<ByteSize>,

    /// Maximum size, e.g. `5M`.
    #[arg(long, value_name = "SIZE")]
    pub max_size: Option<ByteSize>,

    /// Show a long listing.
    #[arg(short, long)]
    pub long: bool,

    /// Emit the results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz ingest`.
#[derive(Debug, Args)]
pub struct IngestArgs {
    /// One or more source URIs.
    #[arg(required = true)]
    pub uris: Vec<String>,

    /// Only ingest objects whose name matches this glob.
    #[arg(long)]
    pub name: Option<String>,

    /// Recurse into directories.
    #[arg(short = 'R', long)]
    pub recursive: bool,

    /// Skip thumbnail generation for media objects.
    #[arg(long)]
    pub no_thumb: bool,

    /// Show a long listing.
    #[arg(short, long)]
    pub long: bool,

    /// Emit the results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz scan`.
#[derive(Debug, Args)]
pub struct ScanArgs {
    /// Only re-check entries whose source URI starts with these prefixes.
    #[arg(value_name = "PREFIX")]
    pub prefixes: Vec<String>,

    /// Skip thumbnail generation for media that has none yet.
    #[arg(long)]
    pub no_thumb: bool,

    /// Emit the results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz forget`.
#[derive(Debug, Args)]
pub struct ForgetArgs {
    /// Target archived entry: an entry id, path, name or source URI.
    #[arg(long = "id", value_name = "TARGET")]
    pub targets: Vec<String>,

    /// Apply to every entry whose path or source starts with this prefix.
    #[arg(long, value_name = "PREFIX")]
    pub prefix: Option<String>,

    /// Emit the results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz sync`.
#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Remote main database; overrides the `main` set in the config.
    #[arg(long)]
    pub remote: Option<String>,

    /// Seed the remote from this node, then exit.
    #[arg(long)]
    pub init: bool,

    /// Allow `--init` to overwrite an existing main database.
    #[arg(long)]
    pub force: bool,

    /// Emit the result as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz open`.
#[derive(Debug, Args)]
pub struct OpenArgs {
    /// Target URI.
    pub uri: String,

    /// Run this command instead of the resolved handler (`{}` is the path).
    #[arg(short = 'w', long)]
    pub with: Option<String>,

    /// Treat the object as this kind instead of detecting it.
    #[arg(long = "as", value_name = "KIND")]
    pub as_kind: Option<String>,

    /// Print the resolved path instead of opening anything.
    #[arg(short, long)]
    pub print: bool,
}

/// Arguments for `d2mz thumb`.
#[derive(Debug, Args)]
pub struct ThumbArgs {
    /// Target URI (must already be ingested so it has a content hash).
    pub uri: String,

    /// Print the thumbnail path instead of displaying it.
    #[arg(short, long)]
    pub print: bool,

    /// Also print the dimensions and format.
    #[arg(short, long)]
    pub long: bool,
}

/// Arguments for `d2mz handler`.
#[derive(Debug, Args)]
pub struct HandlerArgs {
    #[command(subcommand)]
    pub command: HandlerCommand,
}

/// Handler subcommands.
#[derive(Debug, Subcommand)]
pub enum HandlerCommand {
    /// Register or update a handler.
    Set(HandlerSetArgs),
    /// Remove a handler.
    Rm(HandlerRemoveArgs),
    /// List handlers.
    List(HandlerListArgs),
}

/// Arguments for `d2mz handler set`.
#[derive(Debug, Args)]
pub struct HandlerSetArgs {
    /// Kind to handle, e.g. `image` or `video`.
    pub kind: String,

    /// Command to run; `{}` is replaced by the path.
    #[arg(long, value_name = "COMMAND")]
    pub app: String,

    /// Extension this applies to (default `*`, the whole kind).
    #[arg(long, default_value = "*")]
    pub matcher: String,
}

/// Arguments for `d2mz handler rm`.
#[derive(Debug, Args)]
pub struct HandlerRemoveArgs {
    /// Kind to remove from.
    pub kind: String,

    /// Extension to remove (default `*`).
    #[arg(long, default_value = "*")]
    pub matcher: String,
}

/// Arguments for `d2mz handler list`.
#[derive(Debug, Args)]
pub struct HandlerListArgs {
    /// Emit the listing as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz archive`.
#[derive(Debug, Args)]
pub struct ArchiveArgs {
    /// Only show entries under this path prefix.
    #[arg(long)]
    pub prefix: Option<String>,

    /// Show only entries in this state.
    #[arg(long, value_enum)]
    pub state: Option<StateFilter>,

    /// Show a long listing.
    #[arg(short, long)]
    pub long: bool,

    /// Emit the listing as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Presence filter for `archive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum StateFilter {
    /// Sources confirmed present.
    Present,
    /// Sources absent at the last scan.
    Missing,
    /// Entries explicitly retired with `forget`.
    Deleted,
}

/// Arguments for `d2mz export`.
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Blob hash (or unique prefix) to export.
    pub hash: String,

    /// Destination URI. Defaults to the object's recorded source name.
    pub uri: Option<String>,
}

/// Arguments for `d2mz tag`.
#[derive(Debug, Args)]
pub struct TagArgs {
    #[command(subcommand)]
    pub command: TagCommand,
}

/// Tag subcommands.
#[derive(Debug, Subcommand)]
pub enum TagCommand {
    /// Add tags to entries.
    Add(TagAddArgs),
    /// Remove tags from entries.
    Rm(TagRemoveArgs),
    /// List entries carrying a tag, or all tags with no argument.
    List(TagListArgs),
}

/// Arguments for `d2mz tag add`.
#[derive(Debug, Args)]
pub struct TagAddArgs {
    /// Comma-separated tags to attach.
    pub tags: String,

    /// Target archived entry: an entry id, path, name or source URI.
    #[arg(long = "id", value_name = "TARGET")]
    pub targets: Vec<String>,

    /// Apply to every entry whose path starts with this prefix.
    #[arg(long, value_name = "PREFIX")]
    pub prefix: Option<String>,

    /// Emit the results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz tag rm`.
#[derive(Debug, Args)]
pub struct TagRemoveArgs {
    /// Comma-separated tags to detach.
    pub tags: String,

    /// Target archived entry: an entry id, path, name or source URI.
    #[arg(long = "id", value_name = "TARGET")]
    pub targets: Vec<String>,

    /// Apply to every entry whose path starts with this prefix.
    #[arg(long, value_name = "PREFIX")]
    pub prefix: Option<String>,
}

/// Arguments for `d2mz tag list`.
#[derive(Debug, Args)]
pub struct TagListArgs {
    /// Show only entries carrying this tag.
    pub tag: Option<String>,

    /// Show a long listing.
    #[arg(short, long)]
    pub long: bool,

    /// Emit the results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz search`.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// FTS5 query over entry names, paths, source URIs and tags.
    ///
    /// Examples: `report`, `report OR photo`, `"exact phrase"`.
    pub query: String,

    /// Show a long listing.
    #[arg(short, long)]
    pub long: bool,

    /// Emit the results as JSON.
    #[arg(long)]
    pub json: bool,
}
