use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::archive::Archive;
use crate::backend::Backends;
use crate::cli::ExportArgs;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, archive: &Archive, args: ExportArgs) -> Result<()> {
    let hash = resolve_hash(archive, &args.hash)?;

    let bytes = archive.read_blob(&hash)?;
    let dest = match args.uri {
        Some(raw) => Uri::parse(&raw)?,
        None => default_destination(archive, &hash)?,
    };
    let operator = backends.resolve(&dest)?;

    let path = if dest.is_root() {
        bail!("cannot export to a backend root; give a path");
    } else {
        dest.path().to_string()
    };
    operator
        .write(&path, bytes)
        .await
        .with_context(|| format!("writing {dest}"))?;
    println!("exported {hash} -> {dest}");
    Ok(())
}

/// Expand a (possibly abbreviated) hash prefix to a full hash.
fn resolve_hash(archive: &Archive, prefix: &str) -> Result<String> {
    let matches: Vec<String> = archive
        .index()
        .blobs()?
        .into_iter()
        .map(|blob| blob.hash)
        .filter(|hash| hash.starts_with(prefix))
        .collect();

    match matches.len() {
        0 => bail!("no archived blob matches {prefix:?}"),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => bail!("{prefix:?} is ambiguous ({n} blobs match)"),
    }
}

/// Fall back to `local://<basename>` of the most recent entry for the blob.
fn default_destination(archive: &Archive, hash: &str) -> Result<Uri> {
    let entry = archive
        .index()
        .entries()?
        .into_iter()
        .find(|entry| entry.blob == hash)
        .with_context(|| format!("no entry references blob {hash}"))?;
    let name = PathBuf::from(&entry.path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| hash.to_string());
    Uri::parse(&name)
}
