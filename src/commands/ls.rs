use std::env;

use anyhow::{Context, Result};
use opendal::Operator;

use crate::backend::Backends;
use crate::cli::LsArgs;
use crate::output::EntryView;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, args: LsArgs) -> Result<()> {
    let uri = resolve_target(args.uri)?;
    let operator = backends.resolve(&uri)?;

    let views = match single_file(&operator, &uri).await {
        Some(view) => vec![view],
        None => list_children(&operator, &uri).await?,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&views)?);
    } else if args.long {
        for view in &views {
            println!("{}", view.long());
        }
    } else {
        for view in &views {
            println!("{}", view.plain());
        }
    }
    Ok(())
}

/// When the target is a file, return it as a single entry.
async fn single_file(operator: &Operator, uri: &Uri) -> Option<EntryView> {
    if uri.is_root() {
        return None;
    }
    match operator.stat(uri.path()).await {
        Ok(metadata) if metadata.is_file() => Some(EntryView::from_metadata(
            uri.backend(),
            uri.path(),
            &metadata,
        )),
        _ => None,
    }
}

/// List the immediate children of a directory-like target.
async fn list_children(operator: &Operator, uri: &Uri) -> Result<Vec<EntryView>> {
    // A directory without a trailing slash lists the entry itself on some
    // backends; treat the target as a prefix instead.
    let prefix = match uri.path() {
        "" => String::new(),
        path if path.ends_with('/') => path.to_string(),
        path => format!("{path}/"),
    };

    let mut entries = operator
        .list_with(&prefix)
        .recursive(false)
        .await
        .with_context(|| format!("listing {uri}"))?;

    // The target itself is echoed back by some backends; drop it.
    let self_path = prefix.trim_end_matches('/');
    entries.retain(|entry| entry.path().trim_end_matches('/') != self_path);
    entries.sort_by(|a, b| a.path().cmp(b.path()));

    Ok(entries
        .iter()
        .map(|entry| EntryView::from_entry(uri.backend(), entry))
        .collect())
}

/// Use the given URI, or the current directory when omitted.
fn resolve_target(uri: Option<String>) -> Result<Uri> {
    match uri {
        Some(raw) => Uri::parse(&raw),
        None => {
            let cwd = env::current_dir().context("cannot determine current directory")?;
            Uri::parse(&cwd.to_string_lossy())
        }
    }
}
