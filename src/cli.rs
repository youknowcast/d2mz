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

    /// Show a long listing.
    #[arg(short, long)]
    pub long: bool,

    /// Emit the results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Arguments for `d2mz archive`.
#[derive(Debug, Args)]
pub struct ArchiveArgs {
    /// Only show entries under this path prefix.
    #[arg(long)]
    pub prefix: Option<String>,

    /// Show a long listing.
    #[arg(short, long)]
    pub long: bool,

    /// Emit the listing as JSON.
    #[arg(long)]
    pub json: bool,
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
