//! Streaming ingest of objects into the archive.
//!
//! Bytes are hashed with BLAKE3 and written to a temporary file while the
//! hash is computed, then renamed into place. This keeps memory flat for
//! large objects and makes the copy atomic.

use anyhow::{Context, Result};
use blake3::Hasher;
use futures::StreamExt;
use opendal::Operator;
use tokio::io::AsyncWriteExt;

use crate::archive::Archive;
use crate::archive::thumb::{GeneratedThumb, ensure};

/// What an ingest produced.
#[derive(Debug, Clone)]
pub struct IngestOutcome {
    /// BLAKE3 hash of the contents.
    pub hash: String,
    /// Size in bytes.
    pub size: u64,
    /// Whether the blob was already present.
    pub deduplicated: bool,
    /// Whether the source was unchanged and no bytes were read.
    pub unchanged: bool,
    /// Row id of the created entry.
    pub entry_id: i64,
    /// Fully qualified URI of the object that was ingested.
    pub source: String,
    /// A thumbnail, when one was generated or already cached.
    pub thumb: Option<GeneratedThumb>,
}

/// Copy an object from `operator` into the archive.
///
/// When `thumbnails` is set, a thumbnail is generated while the bytes are
/// already in hand, so browsing later never re-reads the source.
///
/// If the index already has an entry for this source whose size and mtime
/// match the backend, and the blob is still on disk, no bytes are read at
/// all: the call is a cheap no-op. This makes re-running `ingest` over a
/// tree incremental without any extra flags.
pub async fn ingest(
    archive: &Archive,
    operator: &Operator,
    backend: &str,
    path: &str,
    name: &str,
    thumbnails: bool,
) -> Result<IngestOutcome> {
    let source = format!("mz://{backend}/{path}");
    let metadata = operator
        .stat(path)
        .await
        .with_context(|| format!("stat {source}"))?;
    if metadata.is_dir() {
        anyhow::bail!("{source} is a directory; ingest individual files");
    }

    let mtime = metadata
        .last_modified()
        .map(|ts| ts.into_inner().as_second());

    // Incremental short-circuit: identical size and mtime, with the blob
    // still on disk, means there is nothing to read or hash.
    if let Some(existing) = archive.index().entry_by_source(&source)?
        && existing.mtime == mtime
        && existing.size == metadata.content_length()
        && blob_is_current(archive, &existing.blob)?
        && existing.state == crate::archive::db::EntryState::Present
    {
        let thumb = if thumbnails {
            ensure(
                archive,
                &existing.blob,
                crate::commands::open::kind_of(name),
            )
            .unwrap_or(None)
        } else {
            None
        };
        return Ok(IngestOutcome {
            hash: existing.blob,
            size: existing.size,
            deduplicated: true,
            unchanged: true,
            entry_id: existing.id,
            source,
            thumb,
        });
    }

    let store = archive.root().join("store");
    std::fs::create_dir_all(&store).context("creating store directory")?;
    let tmp_path = store.join(format!(".tmp.{}.{}", std::process::id(), hash_counter()));

    let reader = operator
        .reader(path)
        .await
        .with_context(|| format!("open {source}"))?;
    let mut stream = reader.into_stream(..).await?;

    let mut hasher = Hasher::new();
    let mut size: u64 = 0;
    {
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .with_context(|| format!("creating {}", tmp_path.display()))?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.with_context(|| format!("reading {source}"))?;
            let bytes = chunk.to_bytes();
            hasher.update(&bytes);
            size += bytes.len() as u64;
            file.write_all(&bytes)
                .await
                .with_context(|| format!("writing {}", tmp_path.display()))?;
        }
        file.flush().await?;
        file.sync_all().await.ok();
    }

    let hash = hasher.finalize().to_hex().to_string();
    let existed = archive.index().has_blob(&hash)?;
    let blob_path = archive.blob_path(&hash);

    if existed && blob_path.exists() {
        let _ = std::fs::remove_file(&tmp_path);
    } else {
        if let Some(parent) = blob_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::rename(&tmp_path, &blob_path)
            .with_context(|| format!("moving blob into {}", blob_path.display()))?;
    }

    let now = unix_now();
    archive.index().insert_blob(&hash, size, now)?;

    // Re-ingesting the same source updates the existing entry instead of
    // creating a duplicate row.
    let entry_id = match archive.index().entry_by_source(&source)? {
        Some(existing) => {
            archive
                .index()
                .mark_present(existing.id, &hash, size, mtime, now)?;
            existing.id
        }
        None => archive
            .index()
            .insert_entry(&hash, name, path, &source, size, now, mtime)?,
    };

    // Generate a thumbnail now, while the source read is already paid for.
    let kind = crate::commands::open::kind_of(name);
    let thumb = if thumbnails {
        ensure(archive, &hash, kind).unwrap_or(None)
    } else {
        None
    };

    Ok(IngestOutcome {
        hash,
        size,
        deduplicated: existed,
        unchanged: false,
        entry_id,
        source,
        thumb,
    })
}

/// Monotonic helper so concurrent ingests do not share a temp name.
fn hash_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Whether the archive still holds the blob on disk.
pub fn blob_is_current(archive: &Archive, hash: &str) -> Result<bool> {
    Ok(archive.has_blob_on_disk(hash))
}

/// Re-ingest the object referenced by an existing entry.
///
/// Used by `scan` when an entry's mtime or size has changed.
pub async fn ingest_from_entry(
    archive: &Archive,
    operator: &Operator,
    entry: &crate::archive::db::Entry,
    backend: &str,
    thumbnails: bool,
) -> Result<IngestOutcome> {
    let path = entry
        .source
        .strip_prefix(&format!("mz://{backend}/"))
        .unwrap_or(&entry.path);
    ingest(archive, operator, backend, path, &entry.name, thumbnails).await
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Flush any buffered writes; exposed for callers that batch ingests.
pub fn flush(_archive: &Archive) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_now_is_positive() {
        assert!(unix_now() > 0);
    }

    #[test]
    fn blob_relative_path_is_sharded() {
        let path = crate::archive::blob_relative_path("abcdef123456");
        assert_eq!(path.to_string_lossy(), "ab/cd/abcdef123456");
    }
}
