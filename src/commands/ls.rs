use std::env;

use anyhow::{Context, Result};
use opendal::Operator;

use crate::backend::Backends;
use crate::browse;
use crate::cli::LsArgs;
use crate::output::EntryView;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, args: LsArgs) -> Result<()> {
    let uri = resolve_target(args.uri)?;
    let operator = backends.resolve(&uri)?;

    let views = match single_file(&operator, &uri).await {
        Some(view) => vec![view],
        None if args.recursive => browse::list_recursive(&operator, &uri).await?,
        None => browse::list_children(&operator, &uri).await?,
    };

    browse::print_views(&views, args.long, args.json)
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
