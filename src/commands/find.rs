use std::env;

use anyhow::{Context, Result};
use globset::{Glob, GlobSetBuilder};

use crate::backend::Backends;
use crate::browse;
use crate::cli::FindArgs;
use crate::uri::Uri;

pub async fn run(backends: &mut Backends, args: FindArgs) -> Result<()> {
    let uri = match &args.uri {
        Some(raw) => Uri::parse(raw)?,
        None => {
            let cwd = env::current_dir().context("cannot determine current directory")?;
            Uri::parse(&cwd.to_string_lossy())?
        }
    };
    let operator = backends.resolve(&uri)?;

    let name = build_glob(args.name.as_deref())?;
    let mut views = browse::list_recursive(&operator, &uri).await?;

    views.retain(|view| {
        if view.kind == "dir" {
            return false;
        }
        if let Some(pattern) = &name
            && !pattern.is_match(&view.name)
        {
            return false;
        }
        if let Some(min) = args.min_size
            && view.size.unwrap_or(0) < min.0
        {
            return false;
        }
        if let Some(max) = args.max_size
            && view.size.unwrap_or(0) > max.0
        {
            return false;
        }
        true
    });

    browse::print_views(&views, args.long, args.json)
}

fn build_glob(pattern: Option<&str>) -> Result<Option<globset::GlobSet>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_pattern_matches_nothing_extra() {
        assert!(build_glob(None).unwrap().is_none());
    }

    #[test]
    fn comma_separated_patterns_are_supported() {
        let set = build_glob(Some("*.txt, *.log")).unwrap().unwrap();
        assert!(set.is_match("a.txt"));
        assert!(set.is_match("b.log"));
        assert!(!set.is_match("c.md"));
    }

    #[test]
    fn invalid_glob_is_rejected() {
        assert!(build_glob(Some("[")).is_err());
    }
}
