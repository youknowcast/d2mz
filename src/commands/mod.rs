//! Subcommand implementations.

pub mod cat;
pub mod find;
pub mod ls;
pub mod stat;

use anyhow::Result;

use crate::backend::Backends;
use crate::cli::{Cli, Command};
use crate::config::Config;

/// Load configuration and dispatch to the requested subcommand.
pub async fn run(cli: Cli) -> Result<()> {
    let config = match &cli.config {
        Some(path) => Config::load_from(path)?,
        None => Config::load()?,
    };
    let mut backends = Backends::new(config);

    match cli.command {
        Command::Ls(args) => ls::run(&mut backends, args).await,
        Command::Cat(args) => cat::run(&mut backends, args).await,
        Command::Stat(args) => stat::run(&mut backends, args).await,
        Command::Find(args) => find::run(&mut backends, args).await,
    }
}
