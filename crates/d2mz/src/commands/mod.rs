//! Subcommand implementations.

pub mod archive;
pub mod cat;
pub mod export;
pub mod find;
pub mod forget;
pub mod generate;
pub mod handler;
pub mod ingest;
pub mod ls;
pub mod open;
pub mod scan;
pub mod search;
pub mod stat;
pub mod sync;
pub mod tag;
pub mod thumb;

use anyhow::Result;

use crate::archive::Archive;
use crate::backend::Backends;
use crate::cli::{Cli, Command};
use crate::config::AppConfig;

/// Load configuration and dispatch to the requested subcommand.
pub async fn run(cli: Cli) -> Result<()> {
    let config = match &cli.config {
        Some(path) => AppConfig::load_from(path)?,
        None => AppConfig::load()?,
    };
    let archive_dir = config.archive_dir.clone();
    let mut backends = Backends::new(config.mz.clone());

    match cli.command {
        Command::Ls(args) => ls::run(&mut backends, args).await,
        Command::Cat(args) => {
            // Open the archive lazily so a vanished source can still be read.
            let archive = Archive::open(&archive_dir).ok();
            cat::run(&mut backends, archive.as_ref(), args).await
        }
        Command::Stat(args) => {
            // Fold in archive metadata (hash, state, tags) when available.
            let archive = Archive::open(&archive_dir).ok();
            stat::run(&mut backends, archive.as_ref(), args).await
        }
        Command::Find(args) => {
            // Open the archive lazily: an already-archived prefix is answered
            // locally, otherwise find walks the backend.
            let archive = Archive::open(&archive_dir).ok();
            find::run(&mut backends, archive.as_ref(), args).await
        }
        Command::Ingest(args) => {
            let archive = Archive::open(&archive_dir)?;
            let result = ingest::run(&mut backends, &archive, args, &config.ignore).await;
            auto_sync(&mut backends, &config, &archive, result.is_ok()).await?;
            result
        }
        Command::Scan(args) => {
            let archive = Archive::open(&archive_dir)?;
            let result = scan::run(&mut backends, &archive, args).await;
            auto_sync(&mut backends, &config, &archive, result.is_ok()).await?;
            result
        }
        Command::Forget(args) => {
            let archive = Archive::open(&archive_dir)?;
            let result = forget::run(&archive, args);
            auto_sync(&mut backends, &config, &archive, result.is_ok()).await?;
            result
        }
        Command::Sync(args) => {
            let archive = Archive::open(&archive_dir)?;
            sync::run(&mut backends, &config, &archive, args).await
        }
        Command::Handler(args) => {
            let archive = Archive::open(&archive_dir)?;
            let result = handler::run(&archive, args);
            auto_sync(&mut backends, &config, &archive, result.is_ok()).await?;
            result
        }
        Command::Open(args) => {
            let archive = Archive::open(&archive_dir)?;
            open::run(&mut backends, &archive, args).await
        }
        Command::Thumb(args) => {
            let archive = Archive::open(&archive_dir)?;
            thumb::run(&mut backends, &archive, args).await
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
            let result = tag::run(&archive, args);
            auto_sync(&mut backends, &config, &archive, result.is_ok()).await?;
            result
        }
        Command::Search(args) => {
            let archive = Archive::open(&archive_dir)?;
            search::run(&archive, args)
        }
        Command::Completions(args) => generate::completions(args),
        Command::Man => generate::man(),
    }
}

/// Push to the main database after an index change, when configured.
///
/// Opt-in via `auto_sync = true` (and a `main`); it never runs when the
/// command itself failed, and its own failure is reported but does not mask
/// the command's result.
async fn auto_sync(
    backends: &mut Backends,
    config: &AppConfig,
    archive: &Archive,
    command_succeeded: bool,
) -> Result<()> {
    if !command_succeeded || !config.auto_sync || config.main.is_none() {
        return Ok(());
    }
    let args = crate::cli::SyncArgs {
        remote: None,
        init: false,
        force: false,
        json: false,
    };
    if let Err(error) = sync::run(backends, config, archive, args).await {
        eprintln!("warning: auto-sync failed: {error}");
    }
    Ok(())
}
