use anyhow::{Context, Result};
use opendal::{Entry, Operator};

use crate::output::EntryView;
use crate::uri::Uri;

/// Render a list of entry views in the requested format.
pub fn print_views(views: &[EntryView], long: bool, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(views)?);
    } else if long {
        for view in views {
            println!("{}", view.long());
        }
    } else {
        for view in views {
            println!("{}", view.plain());
        }
    }
    Ok(())
}

/// The prefix form of a target, so directory-like paths list their children.
pub fn prefix_of(uri: &Uri) -> String {
    match uri.path() {
        "" => String::new(),
        path if path.ends_with('/') => path.to_string(),
        path => format!("{path}/"),
    }
}

/// List the immediate children of a directory-like target.
pub async fn list_children(operator: &Operator, uri: &Uri) -> Result<Vec<EntryView>> {
    let prefix = prefix_of(uri);
    let entries = collect(operator, &prefix, false)
        .await
        .with_context(|| format!("listing {uri}"))?;
    Ok(entries
        .iter()
        .map(|entry| EntryView::from_entry(uri.backend(), entry))
        .collect())
}

/// List every entry below a target, depth-first.
pub async fn list_recursive(operator: &Operator, uri: &Uri) -> Result<Vec<EntryView>> {
    let prefix = prefix_of(uri);
    let entries = collect(operator, &prefix, true)
        .await
        .with_context(|| format!("listing {uri} recursively"))?;
    Ok(entries
        .iter()
        .map(|entry| EntryView::from_entry(uri.backend(), entry))
        .collect())
}

/// List entries under a prefix honouring the self-echo and sorting rules.
async fn collect(operator: &Operator, prefix: &str, recursive: bool) -> Result<Vec<Entry>> {
    let mut entries = operator.list_with(prefix).recursive(recursive).await?;

    // The target itself is echoed back by some backends; drop it.
    let self_path = prefix.trim_end_matches('/');
    entries.retain(|entry| entry.path().trim_end_matches('/') != self_path);
    entries.sort_by(|a, b| a.path().cmp(b.path()));
    Ok(entries)
}
