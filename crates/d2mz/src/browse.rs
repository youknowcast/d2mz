//! Rendering of listings.
//!
//! MZ owns the listing semantics (`mz::browse`); this module turns the raw
//! entries into display views and prints them.

use anyhow::Result;
use opendal::{Entry, Operator};

use crate::output::{EntryView, human_size};
use mz::uri::Uri;

/// Render a list of entry views in the requested format.
pub fn print_views(views: &[EntryView], long: bool, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(views)?);
    } else if long {
        for view in views {
            println!("{}", view.long());
        }
        print_total(views);
    } else {
        for view in views {
            println!("{}", view.plain());
        }
    }
    Ok(())
}

/// Print a `total: N entries, SIZE` trailer for long listings.
///
/// Only shown when there is more than one entry; a single result is its own
/// summary.
fn print_total(views: &[EntryView]) {
    if views.len() < 2 {
        return;
    }
    let bytes: u64 = views.iter().filter_map(|view| view.size).sum();
    let dirs = views.iter().filter(|view| view.kind == "dir").count();
    let files = views.len() - dirs;
    eprintln!(
        "total: {files} file(s), {dirs} dir(s), {}",
        human_size(bytes)
    );
}

/// List the immediate children of a directory-like target as display views.
pub async fn list_children(operator: &Operator, uri: &Uri) -> Result<Vec<EntryView>> {
    let entries = mz::browse::list_children(operator, uri).await?;
    Ok(views_of(uri.backend(), &entries))
}

/// List every entry below a target, depth-first, as display views.
pub async fn list_recursive(operator: &Operator, uri: &Uri) -> Result<Vec<EntryView>> {
    let entries = mz::browse::list_recursive(operator, uri).await?;
    Ok(views_of(uri.backend(), &entries))
}

/// Convert raw entries into display views.
pub fn views_of(backend: &str, entries: &[Entry]) -> Vec<EntryView> {
    entries
        .iter()
        .map(|entry| EntryView::from_entry(backend, entry))
        .collect()
}
