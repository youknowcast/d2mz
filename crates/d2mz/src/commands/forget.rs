use anyhow::{Result, bail};

use crate::cli::ForgetArgs;
use d2mz_archive::Archive;
use d2mz_archive::ingest::unix_now;
use d2mz_archive::list::EntryRecord;

pub fn run(archive: &Archive, args: ForgetArgs) -> Result<()> {
    let targets = resolve_targets(archive, &args.targets, args.prefix.as_deref())?;
    if targets.is_empty() {
        bail!("no entries matched the given target");
    }

    let now = unix_now();
    for entry in &targets {
        archive.index().mark_deleted(entry.id, now)?;
    }

    if args.json {
        let records: Vec<EntryRecord> = targets.iter().map(EntryRecord::from).collect();
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else {
        eprintln!("retired {} entry(ies)", targets.len());
    }
    Ok(())
}

/// Resolve target selectors to entries, mirroring `tag`.
fn resolve_targets(
    archive: &Archive,
    selectors: &[String],
    prefix: Option<&str>,
) -> Result<Vec<d2mz_archive::db::Entry>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(prefix) = prefix {
        for entry in archive.index().entries_matching_prefix(prefix)? {
            if seen.insert(entry.id) {
                out.push(entry);
            }
        }
    }
    for selector in selectors {
        if let Ok(id) = selector.parse::<i64>()
            && let Some(entry) = archive.index().entry(id)?
            && seen.insert(entry.id)
        {
            out.push(entry);
            continue;
        }
        for entry in archive.index().find_entries(selector)? {
            if seen.insert(entry.id) {
                out.push(entry);
            }
        }
    }
    Ok(out)
}
