use anyhow::{Result, bail};

use crate::archive::Archive;
use crate::archive::db::Entry;
use crate::archive::list::EntryRecord;
use crate::cli::SearchArgs;

pub fn run(archive: &Archive, args: SearchArgs) -> Result<()> {
    let entries = resolve(archive, &args.query)?;
    let records: Vec<EntryRecord> = entries.iter().map(EntryRecord::from).collect();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&records)?);
    } else if args.long {
        for record in &records {
            println!("{}", record.long());
        }
    } else {
        for record in &records {
            println!("{}", record.line());
        }
    }

    if records.is_empty() {
        eprintln!(
            "no matches for {:?} (searches ingested entries only)",
            args.query
        );
    }
    Ok(())
}

/// Where a query should be answered from, decided by its shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// A hex blob hash (or prefix).
    Hash(String),
    /// A `#tag` request, or a bare word that is an existing tag.
    Tag(String),
    /// A path-like string (contains `/` or a `.`) to match by prefix.
    Path(String),
    /// Everything else goes to full-text search.
    Text(String),
}

/// Classify a search term so one command covers hash, tag, path and text.
pub fn classify(query: &str, tag_exists: impl Fn(&str) -> bool) -> Query {
    let query = query.trim();

    if let Some(tag) = query.strip_prefix('#')
        && !tag.is_empty()
    {
        return Query::Tag(tag.to_string());
    }

    let is_hex = query.len() >= 4 && query.chars().all(|c| c.is_ascii_hexdigit());
    let looks_like_hash = is_hex
        && query
            .chars()
            .all(|c| !c.is_ascii_uppercase() || c.is_ascii_digit());
    if looks_like_hash && query.len() >= 8 {
        return Query::Hash(query.to_ascii_lowercase());
    }

    if query.contains('/') || (query.contains('.') && !query.contains(' ')) {
        return Query::Path(query.to_string());
    }

    if tag_exists(query) {
        return Query::Tag(query.to_string());
    }

    Query::Text(query.to_string())
}

/// Run the classified query against the index.
fn resolve(archive: &Archive, raw: &str) -> Result<Vec<Entry>> {
    let index = archive.index();
    let query = classify(raw, |tag| index.has_tag(tag).unwrap_or(false));

    match query {
        Query::Hash(prefix) => index.entries_by_blob_prefix(&prefix),
        Query::Tag(tag) => index.entries_with_tag(&tag),
        Query::Path(prefix) => index.entries_matching_prefix(&prefix),
        Query::Text(text) => {
            if text.is_empty() {
                bail!("empty query");
            }
            index.search(&text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_tags(_: &str) -> bool {
        false
    }

    #[test]
    fn classifies_hashes() {
        assert_eq!(
            classify("3f9a1c2bde", no_tags),
            Query::Hash("3f9a1c2bde".into())
        );
        // Too short to be a hash.
        assert_eq!(classify("3f9a", no_tags), Query::Text("3f9a".into()));
    }

    #[test]
    fn classifies_explicit_tags() {
        assert_eq!(classify("#work", no_tags), Query::Tag("work".into()));
    }

    #[test]
    fn classifies_existing_bare_tags() {
        let exists = |tag: &str| tag == "holiday";
        assert_eq!(classify("holiday", exists), Query::Tag("holiday".into()));
        assert_eq!(classify("unknown", exists), Query::Text("unknown".into()));
    }

    #[test]
    fn classifies_paths() {
        assert_eq!(
            classify("docs/report.txt", no_tags),
            Query::Path("docs/report.txt".into())
        );
        assert_eq!(
            classify("report.txt", no_tags),
            Query::Path("report.txt".into())
        );
        assert_eq!(classify("report", no_tags), Query::Text("report".into()));
    }
}
