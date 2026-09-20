//! Listing helpers with MZ semantics.
//!
//! Object stores are flat, so "listing a directory" means listing a prefix.
//! These helpers normalize that: the target is treated as a prefix, the
//! echoed-back self entry is dropped, and directories sort before files.
//! They return raw OpenDAL entries; presentation lives above this layer.

use anyhow::{Context, Result};
use opendal::{Entry, Operator};

use crate::uri::Uri;

/// The prefix form of a target, so directory-like paths list their children.
pub fn prefix_of(uri: &Uri) -> String {
    match uri.path() {
        "" => String::new(),
        path if path.ends_with('/') => path.to_string(),
        path => format!("{path}/"),
    }
}

/// List the immediate children of a directory-like target.
pub async fn list_children(operator: &Operator, uri: &Uri) -> Result<Vec<Entry>> {
    let prefix = prefix_of(uri);
    collect(operator, &prefix, false)
        .await
        .with_context(|| format!("listing {uri}"))
}

/// List every entry below a target, depth-first.
pub async fn list_recursive(operator: &Operator, uri: &Uri) -> Result<Vec<Entry>> {
    let prefix = prefix_of(uri);
    collect(operator, &prefix, true)
        .await
        .with_context(|| format!("listing {uri} recursively"))
}

/// List entries under a prefix honouring the self-echo and sorting rules.
pub async fn collect(operator: &Operator, prefix: &str, recursive: bool) -> Result<Vec<Entry>> {
    let mut entries = operator.list_with(prefix).recursive(recursive).await?;

    // The target itself is echoed back by some backends; drop it.
    let self_path = prefix.trim_end_matches('/');
    entries.retain(|entry| entry.path().trim_end_matches('/') != self_path);

    // Directories first, then by path, matching a normal `ls`.
    entries.sort_by(|a, b| {
        let a_dir = a.metadata().is_dir();
        let b_dir = b.metadata().is_dir();
        b_dir.cmp(&a_dir).then_with(|| a.path().cmp(b.path()))
    });
    Ok(entries)
}
