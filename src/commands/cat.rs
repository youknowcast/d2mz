use anyhow::{Context, Result, bail};
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::backend::Backends;
use crate::cli::CatArgs;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, args: CatArgs) -> Result<()> {
    let uri = Uri::parse(&args.uri)?;
    let operator = backends.resolve(&uri)?;

    let metadata = operator
        .stat(uri.path())
        .await
        .with_context(|| format!("stat {uri}"))?;
    if metadata.is_dir() {
        bail!("{uri} is a directory");
    }

    let reader = operator
        .reader(uri.path())
        .await
        .with_context(|| format!("open {uri}"))?;
    let mut stream = reader.into_stream(..).await?;

    let mut stdout = tokio::io::stdout();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("reading {uri}"))?;
        stdout.write_all(&chunk.to_bytes()).await?;
    }
    stdout.flush().await?;
    Ok(())
}
