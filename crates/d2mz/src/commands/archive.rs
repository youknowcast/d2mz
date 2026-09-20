use anyhow::Result;

use crate::backend::Backends;
use crate::cli::{ArchiveArgs, StateFilter};
use crate::commands::search::{finish, interactive_enabled, pick};
use crate::output::human_size;
use d2mz_archive::Archive;
use d2mz_archive::db::EntryState;
use d2mz_archive::list::EntryRecord;

pub async fn run(backends: &mut Backends, archive: &Archive, args: ArchiveArgs) -> Result<()> {
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

    // On a terminal, narrow interactively instead of dumping the list.
    if interactive_enabled(&args.interactive, args.json)
        && let Some((uri, action)) = pick(&records, &args.interactive)?
    {
        return finish(backends, archive, &uri, action).await;
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else if args.long {
        for record in &records {
            println!("{}", crate::output::entry_long(record));
        }
    } else {
        for record in &records {
            println!("{}", crate::output::entry_line(record));
        }
    }
    if !args.json && records.len() > 1 {
        let bytes: u64 = records.iter().map(|record| record.size).sum();
        eprintln!("total: {} entries, {}", records.len(), human_size(bytes));
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
