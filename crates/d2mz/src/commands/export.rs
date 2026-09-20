use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::backend::Backends;
use crate::cli::ExportArgs;
use crate::uri::Uri;
use d2mz_archive::Archive;
use d2mz_archive::db::EntryState;

pub async fn run(backends: &mut Backends, archive: &Archive, args: ExportArgs) -> Result<()> {
    let hash = resolve_hash(archive, &args.hash)?;
    let bytes = archive.read_blob(&hash)?;

    let dest = match args.uri {
        Some(raw) => Uri::parse(&raw)?,
        // No destination means "restore": put it back where it came from.
        None => restore_target(archive, &hash)?,
    };
    let operator = backends.resolve(&dest)?;

    let path = if dest.is_root() {
        bail!("cannot export to a backend root; give a path");
    } else {
        dest.path().to_string()
    };

    // Restoring over an identical file is a no-op, so a repeated restore is
    // safe and cheap.
    if let Ok(metadata) = operator.stat(&path).await
        && metadata.is_file()
        && metadata.content_length() == bytes.len() as u64
        && operator
            .read(&path)
            .await
            .map(|existing| existing.to_vec() == bytes)
            .unwrap_or(false)
    {
        println!("{dest} already matches {hash}; nothing to do");
        return Ok(());
    }

    operator
        .write(&path, bytes)
        .await
        .with_context(|| format!("writing {dest}"))?;

    // Restore the recorded modification time so a restored tree does not
    // look like it was all written today. Local backends only.
    if dest.backend() == "local"
        && let Some(mtime) = recorded_mtime(archive, &hash)?
    {
        set_local_mtime(&std::path::Path::new("/").join(&path), mtime);
    }

    println!("exported {hash} -> {dest}");
    Ok(())
}

/// The newest recorded mtime for a blob, if any entry has one.
fn recorded_mtime(archive: &Archive, hash: &str) -> Result<Option<i64>> {
    Ok(archive
        .index()
        .entries()?
        .into_iter()
        .find(|entry| entry.blob == hash)
        .and_then(|entry| entry.mtime))
}

/// Set a local file's mtime, ignoring failure (it is best-effort metadata).
fn set_local_mtime(path: &std::path::Path, seconds: i64) {
    let time = filetime::FileTime::from_unix_time(seconds, 0);
    let _ = filetime::set_file_mtime(path, time);
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

/// Where a bare `export` writes: the original source path when it is missing,
/// otherwise the entry's basename in the current directory.
fn restore_target(archive: &Archive, hash: &str) -> Result<Uri> {
    let entries: Vec<_> = archive
        .index()
        .entries()?
        .into_iter()
        .filter(|entry| entry.blob == hash)
        .collect();

    // A missing source is the strongest signal that this is a restore.
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.state == EntryState::Missing)
    {
        return Uri::parse(&entry.source);
    }

    let entry = entries
        .first()
        .with_context(|| format!("no entry references blob {hash}"))?;
    let name = PathBuf::from(&entry.path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| hash.to_string());
    Uri::parse(&name)
}
