use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::archive::Archive;
use crate::archive::db::Entry;
use crate::archive::ingest::{IngestOutcome, ingest, unix_now};
use crate::archive::list::EntryRecord;
use crate::backend::Backends;
use crate::browse;
use crate::cli::IngestArgs;
use crate::uri::Uri;

pub async fn run(
    backends: &mut Backends,
    archive: &Archive,
    args: IngestArgs,
    ignore: &[String],
) -> Result<()> {
    let filter = build_glob(args.name.as_deref())?;
    let ignore = build_patterns(ignore)?;
    let mut outcomes: Vec<IngestOutcome> = Vec::new();
    let mut missing = 0usize;

    for raw in &args.uris {
        let uri = Uri::parse(raw)?;
        let operator = backends.resolve(&uri)?;

        let targets = collect(&operator, &uri, args.recursive, &filter, &ignore).await?;
        if targets.is_empty() {
            anyhow::bail!("no ingestable objects found at {uri}");
        }

        let mut seen = std::collections::HashSet::new();
        for (path, name) in targets {
            seen.insert(format!("mz://{}/{path}", uri.backend()));
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
            if outcome.unchanged {
                eprintln!("unchanged {}", outcome.source);
            } else if outcome.deduplicated {
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

        // A recursive ingest already enumerated everything under the prefix,
        // so it can retire whatever was there before and is now gone.
        if args.recursive {
            missing += sweep_missing(archive, &uri, &seen)?;
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
                unchanged: outcome.unchanged,
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
    // A one-line summary at the end, so a long import is easy to size up.
    summarize(&outcomes);
    if missing > 0 {
        eprintln!("marked {missing} vanished source(s) as missing");
    }
    Ok(())
}

/// Print `N ingested, M deduplicated (saved SIZE), K unchanged` to stderr.
fn summarize(outcomes: &[IngestOutcome]) {
    if outcomes.is_empty() {
        return;
    }
    let ingested = outcomes
        .iter()
        .filter(|o| !o.deduplicated && !o.unchanged)
        .count();
    let deduplicated = outcomes.iter().filter(|o| o.deduplicated).count();
    let unchanged = outcomes.iter().filter(|o| o.unchanged).count();
    let saved: u64 = outcomes
        .iter()
        .filter(|o| o.deduplicated && !o.unchanged)
        .map(|o| o.size)
        .sum();
    eprintln!(
        "{ingested} ingested, {deduplicated} deduplicated (saved {}), {unchanged} unchanged",
        crate::output::human_size(saved)
    );
}

/// Mark entries under `uri` that were not seen in this pass as missing.
///
/// Only sources whose URI falls under the ingested prefix are considered, so
/// an ingest of one subtree never touches another.
fn sweep_missing(
    archive: &Archive,
    uri: &Uri,
    seen: &std::collections::HashSet<String>,
) -> Result<usize> {
    let prefix = format!("mz://{}/{}", uri.backend(), prefix_of(uri.path()));
    let mut count = 0;
    for entry in archive.index().entries_for_prefix(&prefix)? {
        if entry.state == crate::archive::db::EntryState::Deleted {
            continue;
        }
        if !seen.contains(&entry.source) && entry.state != crate::archive::db::EntryState::Missing {
            archive.index().mark_missing(entry.id, unix_now())?;
            count += 1;
        }
    }
    Ok(count)
}

/// The prefix form of a URI, so directory-like paths sweep only below them.
fn prefix_of(path: &str) -> String {
    if path.is_empty() {
        String::new()
    } else if path.ends_with('/') {
        path.to_string()
    } else {
        format!("{path}/")
    }
}

/// Serializable summary of one ingest.
#[derive(Debug, Clone, serde::Serialize)]
struct IngestRecord {
    uri: String,
    path: String,
    hash: String,
    size: u64,
    deduplicated: bool,
    unchanged: bool,
}

/// Resolve the objects to ingest from a URI.
async fn collect(
    operator: &opendal::Operator,
    uri: &Uri,
    recursive: bool,
    filter: &Option<GlobSet>,
    ignore: &Option<GlobSet>,
) -> Result<Vec<(String, String)>> {
    let mut paths = Vec::new();

    if let Some(view) = file_view(operator, uri).await {
        if matches_filter(&view.name, filter) && !is_ignored(&view.path, ignore) {
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
            if matches_filter(&view.name, filter) && !is_ignored(&view.path, ignore) {
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

/// Whether a backend-relative path matches any ignore pattern.
///
/// Matches against both the whole path and its basename, so a plain
/// `.DS_Store` pattern catches the file at any depth.
fn is_ignored(path: &str, ignore: &Option<GlobSet>) -> bool {
    let Some(set) = ignore else {
        return false;
    };
    set.is_match(path) || set.is_match(basename(path))
}

/// Build a glob set from a list of patterns, or `None` when there are none.
fn build_patterns(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder
            .add(Glob::new(pattern).with_context(|| format!("invalid ignore glob {pattern:?}"))?);
    }
    Ok(Some(builder.build().context("building ignore set")?))
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
