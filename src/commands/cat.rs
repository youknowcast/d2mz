use anyhow::{Context, Result, bail};
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::archive::Archive;
use crate::backend::Backends;
use crate::cli::CatArgs;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, archive: Option<&Archive>, args: CatArgs) -> Result<()> {
    let uri = Uri::parse(&args.uri)?;
    let operator = backends.resolve(&uri)?;

    let metadata = operator.stat(uri.path()).await;

    // If the source is gone, serve the archived copy instead of failing.
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == opendal::ErrorKind::NotFound => {
            if let Some(archive) = archive
                && let Some((_, path)) = archive.blob_for_source(&uri.to_string())?
            {
                return stream_file(&path).await;
            }
            return Err(error).with_context(|| format!("stat {uri}"));
        }
        Err(error) => return Err(error).with_context(|| format!("stat {uri}")),
    };
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

/// Stream a local file to stdout, used for the archived fallback.
async fn stream_file(path: &std::path::Path) -> Result<()> {
    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("opening {}", path.display()))?;
    let mut stdout = tokio::io::stdout();
    tokio::io::copy(&mut file, &mut stdout).await?;
    stdout.flush().await?;
    Ok(())
}
