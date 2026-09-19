//! Synchronisation between a local index and a remote "main" index.
//!
//! The remote database is the source of truth but is never used directly:
//! sync downloads it, merges row by row, and pushes local changes back. The
//! merge is last-writer-wins on `updated_at`, with tags and metadata treated
//! as set unions. A lease-based pessimistic lock serialises writers so two
//! nodes cannot merge concurrently.

use anyhow::Result;

use crate::archive::db::{BlobRow, EntryRow, Index};

/// What a merge changed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MergeReport {
    /// Blobs added locally.
    pub blobs_added: usize,
    /// Entries inserted or updated locally.
    pub entries_changed: usize,
    /// Entries skipped because the local copy was newer.
    pub entries_skipped: usize,
    /// Tags added locally.
    pub tags_added: usize,
}

/// A snapshot exchanged with the remote database.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    /// Identity of the main database this snapshot belongs to.
    ///
    /// Assigned on `--init` and preserved on every write. A node remembers
    /// the id it first synced with so pointing at a *different* main
    /// database is caught instead of silently merging two catalogues.
    #[serde(default)]
    pub main_id: String,
    /// All blob rows.
    pub blobs: Vec<BlobRow>,
    /// All entry rows.
    pub entries: Vec<EntryRow>,
    /// All (source, tag) pairs.
    pub tags: Vec<(String, String)>,
    /// All (source, key, value) triples.
    pub meta: Vec<(String, String, String)>,
}

impl Snapshot {
    /// Capture the full contents of an index.
    pub fn capture(index: &Index) -> Result<Snapshot> {
        Ok(Snapshot {
            main_id: String::new(),
            blobs: index.blob_rows()?,
            entries: index.entry_rows()?,
            tags: index.tag_rows()?,
            meta: index.meta_rows()?,
        })
    }

    /// Merge a remote snapshot into `index` (last-writer-wins).
    pub fn merge_into(&self, index: &Index) -> Result<MergeReport> {
        let mut report = MergeReport::default();

        let known: std::collections::HashSet<String> = index
            .blob_rows()?
            .into_iter()
            .map(|blob| blob.hash)
            .collect();
        for blob in &self.blobs {
            index.upsert_blob_row(blob)?;
            if !known.contains(&blob.hash) {
                report.blobs_added += 1;
            }
        }

        for entry in &self.entries {
            if index.upsert_entry_row(entry)? {
                report.entries_changed += 1;
            } else {
                report.entries_skipped += 1;
            }
        }

        for (source, tag) in &self.tags {
            if index.add_tag_by_source(source, tag)? {
                report.tags_added += 1;
            }
        }

        for (source, key, value) in &self.meta {
            if let Some(entry) = index.entry_by_source(source)? {
                index.set_meta(entry.id, key, value)?;
            }
        }

        Ok(report)
    }
}

/// Compute what the local index has that the remote snapshot does not.
///
/// Used to decide what to push after a pull.
pub fn outgoing(index: &Index, remote: &Snapshot) -> Result<Snapshot> {
    let local = Snapshot::capture(index)?;

    let remote_blobs: std::collections::HashSet<&str> =
        remote.blobs.iter().map(|blob| blob.hash.as_str()).collect();
    let remote_entries: std::collections::HashMap<&str, &EntryRow> = remote
        .entries
        .iter()
        .map(|entry| (entry.source.as_str(), entry))
        .collect();

    let blobs: Vec<BlobRow> = local
        .blobs
        .into_iter()
        .filter(|blob| !remote_blobs.contains(blob.hash.as_str()))
        .collect();

    let entries: Vec<EntryRow> = local
        .entries
        .into_iter()
        .filter(|entry| match remote_entries.get(entry.source.as_str()) {
            Some(remote_entry) => entry.updated_at > remote_entry.updated_at,
            None => true,
        })
        .collect();

    let remote_tags: std::collections::HashSet<&(String, String)> = remote.tags.iter().collect();
    let tags: Vec<(String, String)> = local
        .tags
        .into_iter()
        .filter(|pair| !remote_tags.contains(pair))
        .collect();

    let remote_meta: std::collections::HashSet<&(String, String, String)> =
        remote.meta.iter().collect();
    let meta: Vec<(String, String, String)> = local
        .meta
        .into_iter()
        .filter(|triple| !remote_meta.contains(triple))
        .collect();

    Ok(Snapshot {
        main_id: remote.main_id.clone(),
        blobs,
        entries,
        tags,
        meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> Index {
        let index = Index::open_in_memory().unwrap();
        index.insert_blob("aaa", 3, 10).unwrap();
        index
            .insert_entry("aaa", "a.txt", "a.txt", "mz://local/a.txt", 3, 10, Some(5))
            .unwrap();
        index
    }

    #[test]
    fn capture_round_trips() {
        let index = seeded();
        index.add_tag(1, "work").unwrap();
        let snapshot = Snapshot::capture(&index).unwrap();
        assert_eq!(snapshot.blobs.len(), 1);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(
            snapshot.tags,
            vec![("mz://local/a.txt".into(), "work".into())]
        );
    }

    #[test]
    fn merge_adds_missing_rows() {
        let source = seeded();
        let target = Index::open_in_memory().unwrap();
        let snapshot = Snapshot::capture(&source).unwrap();

        let report = snapshot.merge_into(&target).unwrap();
        assert_eq!(report.blobs_added, 1);
        assert_eq!(report.entries_changed, 1);
        assert_eq!(target.entry_count().unwrap(), 1);
    }

    #[test]
    fn merge_is_last_writer_wins() {
        let source = seeded();
        let target = seeded();

        // Bump the target's entry so it is newer than the snapshot.
        target.mark_present(1, "aaa", 3, Some(5), 100).unwrap();
        let snapshot = Snapshot::capture(&source).unwrap();
        let report = snapshot.merge_into(&target).unwrap();
        assert_eq!(report.entries_skipped, 1);

        let entry = target.entry(1).unwrap().unwrap();
        assert_eq!(entry.seen_at, Some(100));
    }

    #[test]
    fn outgoing_excludes_rows_already_remote() {
        let local = seeded();
        local.add_tag(1, "work").unwrap();
        let remote = Snapshot::capture(&local).unwrap();

        let outgoing = outgoing(&local, &remote).unwrap();
        assert!(outgoing.blobs.is_empty());
        assert!(outgoing.entries.is_empty());
        assert!(outgoing.tags.is_empty());
    }

    #[test]
    fn outgoing_includes_newer_local_entries() {
        let local = seeded();
        let remote = Snapshot::capture(&local).unwrap();
        local.mark_present(1, "aaa", 3, Some(9), 999).unwrap();

        let outgoing = outgoing(&local, &remote).unwrap();
        assert_eq!(outgoing.entries.len(), 1);
        assert_eq!(outgoing.entries[0].updated_at, 999);
    }
}
