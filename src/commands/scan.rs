use anyhow::{Context, Result};

use crate::archive::Archive;
use crate::archive::db::EntryState;
use crate::archive::ingest::{blob_is_current, ingest_from_entry, unix_now};
use crate::backend::Backends;
use crate::cli::ScanArgs;
use crate::uri::Uri;

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
            Err(_) => {
                archive.index().mark_missing(entry.id, unix_now())?;
                ("missing", false)
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
                    if args.thumb {
                        let kind = crate::commands::open::kind_of(&entry.name);
                        let _ = crate::archive::thumb::ensure(archive, &entry.blob, kind);
                    }
                    ("present", false)
                } else {
                    ingest_from_entry(archive, &operator, &entry, uri.backend(), args.thumb)
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
        let changed = records.iter().filter(|r| r.changed).count();
        eprintln!(
            "scanned {}: {present} present, {missing} missing, {changed} changed",
            records.len()
        );
    }
    Ok(())
}
