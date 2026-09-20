use anyhow::{Context, Result, bail};

use crate::archive::Archive;
use crate::archive::lock::{DEFAULT_TTL_SECS, acquire};
use crate::archive::sync::{Snapshot, outgoing};
use crate::backend::Backends;
use crate::cli::SyncArgs;
use crate::config::AppConfig;
use crate::uri::Uri;

/// Result of a sync run, printed as text or JSON.
#[derive(Debug, serde::Serialize)]
struct SyncReport {
    remote: String,
    pulled_blobs: usize,
    pulled_entries: usize,
    pushed_entries: usize,
    tags_added: usize,
}

pub async fn run(
    backends: &mut Backends,
    config: &AppConfig,
    archive: &Archive,
    args: SyncArgs,
) -> Result<()> {
    let remote = resolve_remote(config, &args)?;
    let remote_uri = Uri::parse(&remote)?;
    let operator = backends.resolve(&remote_uri)?;
    let lock_path = lock_path_for(remote_uri.path());

    // `sync --init` seeds the remote from this node and stops there.
    if args.init {
        return init_remote(
            backends,
            archive,
            &remote,
            &operator,
            remote_uri.path(),
            &lock_path,
            args.force,
        )
        .await;
    }

    // Download the remote snapshot before locking; it is read-only work.
    let remote_snapshot = read_remote(&operator, remote_uri.path()).await?;

    // Refuse to merge with a different main database than the one bound here.
    check_main_id(archive, &remote_snapshot, &remote)?;

    let guard = acquire(
        &operator,
        &lock_path,
        archive.index().node(),
        &hostname(),
        DEFAULT_TTL_SECS,
    )
    .await
    .context("acquiring the remote sync lock")?;

    // Merge remote changes into the local index.
    let merged = remote_snapshot.merge_into(archive.index())?;

    // Then push anything the remote is missing or behind on.
    let outbound = outgoing(archive.index(), &remote_snapshot)?;
    let pushed_entries = outbound.entries.len();
    let pushed = merge_remote(&remote_snapshot, &outbound);
    write_remote(backends, &remote, &pushed).await?;

    guard.release().await.ok();

    let report = SyncReport {
        remote: remote.clone(),
        pulled_blobs: merged.blobs_added,
        pulled_entries: merged.entries_changed,
        pushed_entries,
        tags_added: merged.tags_added,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        eprintln!(
            "synced {remote}: pulled {} blobs / {} entries, pushed {} entries, {} tags added",
            report.pulled_blobs, report.pulled_entries, report.pushed_entries, report.tags_added
        );
    }
    Ok(())
}

/// Create or, with `--force`, overwrite the main database.
async fn init_remote(
    backends: &mut Backends,
    archive: &Archive,
    remote: &str,
    operator: &opendal::Operator,
    path: &str,
    lock_path: &str,
    force: bool,
) -> Result<()> {
    let existing = read_remote(operator, path).await?;
    if !existing.entries.is_empty() && !existing.main_id.is_empty() && !force {
        bail!(
            "main database {remote} already exists (id {}); pass --force to overwrite it",
            existing.main_id
        );
    }

    // Acquire the lock so a concurrent writer cannot race the seed.
    let guard = acquire(
        operator,
        lock_path,
        archive.index().node(),
        &hostname(),
        DEFAULT_TTL_SECS,
    )
    .await
    .context("acquiring the remote sync lock")?;

    let main_id = if force || existing.main_id.is_empty() {
        new_main_id(archive)
    } else {
        existing.main_id.clone()
    };

    let mut snapshot = Snapshot::capture(archive.index())?;
    snapshot.main_id = main_id.clone();
    write_remote(backends, remote, &snapshot).await?;
    archive.index().bind_main_id(&main_id)?;

    guard.release().await.ok();
    eprintln!("initialised remote {remote} (main id {main_id}) from this node");
    Ok(())
}

/// Ensure the remote belongs to the main database this node is bound to.
fn check_main_id(archive: &Archive, remote: &Snapshot, remote_uri: &str) -> Result<()> {
    if remote.main_id.is_empty() {
        // Not yet initialised; nothing to bind to.
        return Ok(());
    }
    match archive.index().bound_main_id()? {
        Some(bound) if bound != remote.main_id => bail!(
            "{remote_uri} is a different main database (id {}), but this node is bound to {bound}",
            remote.main_id
        ),
        Some(_) => Ok(()),
        None => {
            // First contact: bind to it so future mismatches are caught.
            archive.index().bind_main_id(&remote.main_id)?;
            Ok(())
        }
    }
}

/// Derive a fresh main-database id from the node and the clock.
fn new_main_id(archive: &Archive) -> String {
    format!("main-{}", archive.index().node())
}

/// Merge local-only changes into the remote snapshot before writing it back.
fn merge_remote(remote: &Snapshot, added: &Snapshot) -> Snapshot {
    let mut merged = remote.clone();

    for blob in &added.blobs {
        if !merged.blobs.iter().any(|b| b.hash == blob.hash) {
            merged.blobs.push(blob.clone());
        }
    }
    for entry in &added.entries {
        match merged.entries.iter_mut().find(|e| e.source == entry.source) {
            Some(slot) => *slot = entry.clone(),
            None => merged.entries.push(entry.clone()),
        }
    }
    for tag in &added.tags {
        if !merged.tags.contains(tag) {
            merged.tags.push(tag.clone());
        }
    }
    for meta in &added.meta {
        if !merged.meta.contains(meta) {
            merged.meta.push(meta.clone());
        }
    }
    for handler in &added.handlers {
        match merged
            .handlers
            .iter_mut()
            .find(|h| h.kind == handler.kind && h.matcher == handler.matcher)
        {
            Some(slot) => *slot = handler.clone(),
            None => merged.handlers.push(handler.clone()),
        }
    }
    merged
}

/// Where the remote snapshot comes from: `--remote`, else `main` in config.
fn resolve_remote(config: &AppConfig, args: &SyncArgs) -> Result<String> {
    if let Some(remote) = &args.remote {
        return Ok(remote.clone());
    }
    if let Some(main) = &config.main {
        return Ok(main.clone());
    }
    bail!("no main database configured; set `main = \"mz://...\"` or pass --remote")
}

/// The lock object lives next to the database file.
fn lock_path_for(db_path: &str) -> String {
    let base = db_path.trim_end_matches(".sqlite3").trim_end_matches(".db");
    format!("{base}.lock.json")
}

/// Download and parse the remote snapshot.
async fn read_remote(operator: &opendal::Operator, path: &str) -> Result<Snapshot> {
    match operator.read(path).await {
        Ok(buffer) => Ok(serde_json::from_slice(&buffer.to_vec())
            .with_context(|| format!("parsing remote snapshot {path}"))?),
        Err(error) if error.kind() == opendal::ErrorKind::NotFound => Ok(Snapshot::default()),
        Err(error) => Err(error).with_context(|| format!("reading remote snapshot {path}")),
    }
}

/// Upload the merged snapshot as a portable JSON document.
async fn write_remote(backends: &mut Backends, remote: &str, snapshot: &Snapshot) -> Result<()> {
    let uri = Uri::parse(remote)?;
    let operator = backends.resolve(&uri)?;
    let body = serde_json::to_vec_pretty(snapshot)?;
    operator
        .write(uri.path(), body)
        .await
        .with_context(|| format!("writing remote snapshot {remote}"))?;
    Ok(())
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".to_string())
}
