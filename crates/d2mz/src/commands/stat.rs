use anyhow::{Context, Result};

use crate::backend::Backends;
use crate::cli::StatArgs;
use crate::output::ObjectView;
use crate::uri::Uri;
use d2mz_archive::Archive;

pub async fn run(backends: &mut Backends, archive: Option<&Archive>, args: StatArgs) -> Result<()> {
    let uri = Uri::parse(&args.uri)?;
    let operator = backends.resolve(&uri)?;

    let metadata = operator
        .stat(uri.path())
        .await
        .with_context(|| format!("stat {uri}"))?;
    let mut view = ObjectView::from_metadata(uri.backend(), uri.path(), &metadata);

    // Fold in what the archive knows, so "is this ingested?" needs no
    // second command.
    if let Some(archive) = archive
        && let Some(entry) = archive.index().entry_by_source(&uri.to_string())?
    {
        let tags = archive.index().tags_of(entry.id)?;
        view = view.with_archive(&entry, tags);
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!("{}", view.verbose());
    }
    Ok(())
}
