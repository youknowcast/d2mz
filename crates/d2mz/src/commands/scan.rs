use anyhow::{Context, Result};

use crate::backend::Backends;
use crate::cli::ScanArgs;
use crate::uri::Uri;
use d2mz_archive::Archive;
use d2mz_archive::db::EntryState;
use d2mz_archive::ingest::{blob_is_current, ingest_from_entry, unix_now};

/// Summary of one scan.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanRecord {
    pub source: String,
    pub state: &'static str,
    pub changed: bool,
}

pub async fn run(backends: &mut Backends, archive: &Archive, args: ScanArgs) -> Result<()> {
    let entries = if args.prefixes.is_empty() {
        archive.index().entries()?
    } else {
        let mut all = Vec::new();
        for prefix in &args.prefixes {
            all.extend(archive.index().entries_for_prefix(prefix)?);
        }
        all
    };

    let mut records = Vec::new();
    for entry in entries {
        if entry.state == EntryState::Deleted {
            continue;
        }
        let uri = Uri::parse(&entry.source)
            .with_context(|| format!("parsing recorded source {}", entry.source))?;
        let operator = backends.resolve(&uri)?;

        let metadata = operator.stat(uri.path()).await;
        let (state, changed) = match metadata {
            // Only a genuine "not found" means the source is gone. A
            // transport or auth failure must not mark everything missing, so
            // leave the recorded state untouched and report it.
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => {
                archive.index().mark_missing(entry.id, unix_now())?;
                ("missing", false)
            }
            Err(error) => {
                records.push(ScanRecord {
                    source: entry.source.clone(),
                    state: "error",
                    changed: false,
                });
                eprintln!("warning: could not check {}: {error}", entry.source);
                continue;
            }
            Ok(metadata) if metadata.is_dir() => {
                archive.index().mark_missing(entry.id, unix_now())?;
                ("missing", false)
            }
            Ok(metadata) => {
                let mtime = metadata
                    .last_modified()
                    .map(|ts| ts.into_inner().as_second());
                let same = entry.mtime == mtime
                    && entry.size == metadata.content_length()
                    && blob_is_current(archive, &entry.blob)?;
                if same {
                    archive.index().mark_present(
                        entry.id,
                        &entry.blob,
                        entry.size,
                        mtime,
                        unix_now(),
                    )?;
                    // `--thumb` backfills thumbnails for already-ingested
                    // media, reading the blob from the local store.
                    if !args.no_thumb {
                        let kind = d2mz_archive::media::kind_of(&entry.name);
                        let _ = d2mz_archive::thumb::ensure(archive, &entry.blob, kind);
                    }
                    ("present", false)
                } else {
                    ingest_from_entry(archive, &operator, &entry, uri.backend(), !args.no_thumb)
                        .await?;
                    ("present", true)
                }
            }
        };
        records.push(ScanRecord {
            source: entry.source.clone(),
            state,
            changed,
        });
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else {
        let present = records.iter().filter(|r| r.state == "present").count();
        let missing = records.iter().filter(|r| r.state == "missing").count();
        let errors = records.iter().filter(|r| r.state == "error").count();
        let changed = records.iter().filter(|r| r.changed).count();
        eprintln!(
            "scanned {}: {present} present, {missing} missing, {changed} changed, {errors} unchecked",
            records.len()
        );
    }
    Ok(())
}
