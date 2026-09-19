use anyhow::Result;

use crate::archive::Archive;
use crate::archive::list::EntryRecord;
use crate::cli::SearchArgs;

pub fn run(archive: &Archive, args: SearchArgs) -> Result<()> {
    let entries = archive.index().search(&args.query)?;
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

    if records.is_empty() {
        eprintln!(
            "no matches for {:?} (searches ingested entries only)",
            args.query
        );
    }
    Ok(())
}
