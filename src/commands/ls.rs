use std::env;

use anyhow::{Context, Result};

use crate::backend::Backends;
use crate::cli::LsArgs;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, args: LsArgs) -> Result<()> {
    let uri = resolve_target(args.uri)?;
    let operator = backends.resolve(&uri)?;

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

    for entry in entries {
        println!("{}", entry.name());
    }
    Ok(())
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
