use anyhow::Result;

use crate::archive::Archive;
use crate::archive::db::EntryState;
use crate::archive::list::EntryRecord;
use crate::cli::{ArchiveArgs, StateFilter};
use crate::output::human_size;

pub fn run(archive: &Archive, args: ArchiveArgs) -> Result<()> {
    let state = args.state.map(to_state);
    let mut entries = match (state, &args.prefix) {
        (Some(state), _) => archive.index().entries_in_state(state)?,
        (None, Some(prefix)) => archive.index().entries_under(prefix)?,
        (None, None) => archive.index().entries()?,
    };

    // Retired entries are tombstones; keep them out of the way unless their
    // state is asked for explicitly.
    if state != Some(EntryState::Deleted) {
        entries.retain(|entry| entry.state != EntryState::Deleted);
    }

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

fn to_state(filter: StateFilter) -> EntryState {
    match filter {
        StateFilter::Present => EntryState::Present,
        StateFilter::Missing => EntryState::Missing,
        StateFilter::Deleted => EntryState::Deleted,
    }
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
