use anyhow::Result;

use crate::archive::Archive;
use crate::archive::list::EntryRecord;
use crate::cli::ArchiveArgs;
use crate::output::human_size;

pub fn run(archive: &Archive, args: ArchiveArgs) -> Result<()> {
    let entries = match &args.prefix {
        Some(prefix) => archive.index().entries_under(prefix)?,
        None => archive.index().entries()?,
    };
    let records: Vec<EntryRecord> = entries.iter().map(EntryRecord::from).collect();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else if args.long {
        for record in &records {
            println!("{}", record.long());
        }
    } else {
        for record in &records {
            println!("{}", record.line());
        }
    }
    Ok(())
}

/// Summary line printed by `ingest` and `archive` when requested.
pub fn summary(archive: &Archive) -> Result<String> {
    let index = archive.index();
    Ok(format!(
        "{} blobs ({}), {} entries -> {}",
        index.blob_count()?,
        human_size(index.blob_bytes()? as u64),
        index.entry_count()?,
        archive.root().display()
    ))
}
