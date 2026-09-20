use clap::{CommandFactory, FromArgMatches};

use d2mz::cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `version` is set at runtime so the embedded git revision can appear in
    // `d2mz --version` (`0.8.0+abcd123`).
    let matches = Cli::command().version(d2mz::VERSION).get_matches();
    let cli = Cli::from_arg_matches(&matches).expect("clap validated the arguments");
    d2mz::init_http();
    d2mz::commands::run(cli).await
}
