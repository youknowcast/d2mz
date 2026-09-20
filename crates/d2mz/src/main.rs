use clap::Parser;

use d2mz::cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    d2mz::init_http();
    d2mz::commands::run(cli).await
}
