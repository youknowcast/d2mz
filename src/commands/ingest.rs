use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::archive::Archive;
use crate::archive::db::Entry;
use crate::archive::ingest::{IngestOutcome, ingest};
use crate::archive::list::EntryRecord;
use crate::backend::Backends;
use crate::browse;
use crate::cli::IngestArgs;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, archive: &Archive, args: IngestArgs) -> Result<()> {
    let filter = build_glob(args.name.as_deref())?;
    let mut outcomes: Vec<IngestOutcome> = Vec::new();

    for raw in &args.uris {
        let uri = Uri::parse(raw)?;
        let operator = backends.resolve(&uri)?;

        let targets = collect(&operator, &uri, args.recursive, &filter).await?;
        if targets.is_empty() {
            anyhow::bail!("no ingestable objects found at {uri}");
        }
        for (path, name) in targets {
            let outcome = ingest(
                archive,
                &operator,
                uri.backend(),
                &path,
                &name,
                !args.no_thumb,
            )
            .await?;
            let short = &outcome.hash[..12.min(outcome.hash.len())];
            if outcome.deduplicated {
                eprintln!(
                    "deduplicated {} (blob {short} already present)",
                    outcome.source
                );
            } else {
                eprintln!(
                    "ingested {} ({} -> new blob, {} bytes)",
                    outcome.source, short, outcome.size
                );
            }
            outcomes.push(outcome);
        }
    }

    if args.json || args.long {
        let records: Vec<IngestRecord> = outcomes
            .iter()
            .map(|outcome| IngestRecord {
                uri: outcome.source.clone(),
                path: outcome.source.clone(),
                hash: outcome.hash.clone(),
                size: outcome.size,
                deduplicated: outcome.deduplicated,
            })
            .collect();
        if args.json {
            println!("{}", serde_json::to_string_pretty(&records)?);
        } else {
            for record in &records {
                println!(
                    "{} {:>8}  {}",
                    &record.hash[..12.min(record.hash.len())],
                    crate::output::human_size(record.size),
                    record.uri
                );
            }
        }
    }
    Ok(())
}

/// Serializable summary of one ingest.
#[derive(Debug, Clone, serde::Serialize)]
struct IngestRecord {
    uri: String,
    path: String,
    hash: String,
    size: u64,
    deduplicated: bool,
}

/// Resolve the objects to ingest from a URI.
async fn collect(
    operator: &opendal::Operator,
    uri: &Uri,
    recursive: bool,
    filter: &Option<GlobSet>,
) -> Result<Vec<(String, String)>> {
    let mut paths = Vec::new();

    if let Some(view) = file_view(operator, uri).await {
        if matches_filter(&view.name, filter) {
            paths.push(view.path);
        }
    } else {
        let views = if recursive {
            browse::list_recursive(operator, uri).await?
        } else {
            browse::list_children(operator, uri).await?
        };
        for view in views {
            if view.kind == "dir" {
                continue;
            }
            if matches_filter(&view.name, filter) {
                paths.push(view.path);
            }
        }
    }

    Ok(paths
        .into_iter()
        .map(|path| {
            let name = basename(&path);
            (path, name)
        })
        .collect())
}

/// A file target resolves to itself.
async fn file_view(operator: &opendal::Operator, uri: &Uri) -> Option<crate::output::EntryView> {
    if uri.is_root() {
        return None;
    }
    match operator.stat(uri.path()).await {
        Ok(metadata) if metadata.is_file() => Some(crate::output::EntryView::from_metadata(
            uri.backend(),
            uri.path(),
            &metadata,
        )),
        _ => None,
    }
}

fn matches_filter(name: &str, filter: &Option<GlobSet>) -> bool {
    match filter {
        Some(set) => set.is_match(name),
        None => true,
    }
}

fn basename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn build_glob(pattern: Option<&str>) -> Result<Option<GlobSet>> {
    let Some(pattern) = pattern else {
        return Ok(None);
    };
    let mut builder = GlobSetBuilder::new();
    for candidate in pattern.split(',') {
        let candidate = candidate.trim();
        if candidate.is_empty() {
            continue;
        }
        builder.add(Glob::new(candidate).with_context(|| format!("invalid glob {candidate:?}"))?);
    }
    Ok(Some(builder.build().context("building glob set")?))
}

/// Convenience for tests and callers: render archived entries.
pub fn records(entries: &[Entry]) -> Vec<EntryRecord> {
    entries.iter().map(EntryRecord::from).collect()
}
