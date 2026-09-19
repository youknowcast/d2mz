//! Subcommand implementations.

pub mod archive;
pub mod cat;
pub mod export;
pub mod find;
pub mod forget;
pub mod generate;
pub mod ingest;
pub mod ls;
pub mod scan;
pub mod search;
pub mod stat;
pub mod sync;
pub mod tag;

use anyhow::Result;

use crate::archive::Archive;
use crate::backend::Backends;
use crate::cli::{Cli, Command};
use crate::config::Config;

/// Load configuration and dispatch to the requested subcommand.
pub async fn run(cli: Cli) -> Result<()> {
    let config = match &cli.config {
        Some(path) => Config::load_from(path)?,
        None => Config::load()?,
    };
    let archive_dir = config.archive_dir.clone();
    let mut backends = Backends::new(config.clone());

    match cli.command {
        Command::Ls(args) => ls::run(&mut backends, args).await,
        Command::Cat(args) => cat::run(&mut backends, args).await,
        Command::Stat(args) => stat::run(&mut backends, args).await,
        Command::Find(args) => find::run(&mut backends, args).await,
        Command::Ingest(args) => {
            let archive = Archive::open(&archive_dir)?;
            ingest::run(&mut backends, &archive, args).await
        }
        Command::Scan(args) => {
            let archive = Archive::open(&archive_dir)?;
            scan::run(&mut backends, &archive, args).await
        }
        Command::Forget(args) => {
            let archive = Archive::open(&archive_dir)?;
            forget::run(&archive, args)
        }
        Command::Sync(args) => {
            let archive = Archive::open(&archive_dir)?;
            sync::run(&mut backends, &config, &archive, args).await
        }
        Command::Archive(args) => {
            let archive = Archive::open(&archive_dir)?;
            archive::run(&archive, args)
        }
        Command::Export(args) => {
            let archive = Archive::open(&archive_dir)?;
            export::run(&mut backends, &archive, args).await
        }
        Command::Tag(args) => {
            let archive = Archive::open(&archive_dir)?;
            tag::run(&archive, args)
        }
        Command::Search(args) => {
            let archive = Archive::open(&archive_dir)?;
            search::run(&archive, args)
        }
        Command::Completions(args) => generate::completions(args),
        Command::Man => generate::man(),
    }
}
