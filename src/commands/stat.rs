use anyhow::{Context, Result};

use crate::backend::Backends;
use crate::cli::StatArgs;
use crate::output::ObjectView;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, args: StatArgs) -> Result<()> {
    let uri = Uri::parse(&args.uri)?;
    let operator = backends.resolve(&uri)?;

    let metadata = operator
        .stat(uri.path())
        .await
        .with_context(|| format!("stat {uri}"))?;
    let view = ObjectView::from_metadata(uri.backend(), uri.path(), &metadata);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!("{}", view.verbose());
    }
    Ok(())
}
