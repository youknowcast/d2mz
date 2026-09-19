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
