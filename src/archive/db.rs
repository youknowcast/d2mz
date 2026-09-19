//! SQLite-backed metadata index for the archive.
//!
//! The schema separates *blobs* (unique contents, keyed by BLAKE3 hash) from
//! *entries* (named sightings of a blob), which is what makes deduplication
//! and multiple names per file natural.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

/// A named entry recorded in the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Row id.
    pub id: i64,
    /// BLAKE3 hash of the contents.
    pub blob: String,
    /// Display name.
    pub name: String,
    /// Backend-relative path at the time of ingest.
    pub path: String,
    /// URI the bytes came from.
    pub source: String,
    /// Size in bytes.
    pub size: u64,
    /// Unix timestamp of ingest.
    pub created_at: i64,
}

/// Summary of a blob and how many entries reference it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobStat {
    /// BLAKE3 hash.
    pub hash: String,
    /// Size in bytes.
    pub size: u64,
    /// Number of entries pointing at this blob.
    pub entries: i64,
}

/// A tag and how many entries carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagStat {
    /// Tag text.
    pub tag: String,
    /// Number of entries carrying the tag.
    pub entries: i64,
}

/// The archive metadata index.
pub struct Index {
    conn: Connection,
}

impl Index {
    /// Open (or create) the index database at `path`.
    pub fn open(path: &Path) -> Result<Index> {
        let conn =
            Connection::open(path).with_context(|| format!("opening index {}", path.display()))?;
        let index = Index { conn };
        index.migrate()?;
        Ok(index)
    }

    /// Open an in-memory index, for tests.
    pub fn open_in_memory() -> Result<Index> {
        let index = Index {
            conn: Connection::open_in_memory()?,
        };
        index.migrate()?;
        Ok(index)
    }

    fn migrate(&self) -> Result<()> {
        self.conn
            .execute_batch(
                "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS blob (
                hash       TEXT PRIMARY KEY,
                size       INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS entry (
                id         INTEGER PRIMARY KEY,
                blob_hash  TEXT NOT NULL REFERENCES blob(hash),
                name       TEXT NOT NULL,
                path       TEXT NOT NULL,
                source     TEXT NOT NULL,
                size       INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS entry_blob ON entry(blob_hash);

            CREATE TABLE IF NOT EXISTS tag (
                entry_id INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
                tag      TEXT NOT NULL,
                PRIMARY KEY (entry_id, tag)
            );

            CREATE INDEX IF NOT EXISTS tag_name ON tag(tag);

            CREATE TABLE IF NOT EXISTS meta (
                entry_id INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
                key      TEXT NOT NULL,
                value    TEXT NOT NULL,
                PRIMARY KEY (entry_id, key)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS entry_fts USING fts5(
                name, path, source, tags,
                tokenize='unicode61'
            );
            ",
            )
            .context("applying schema")?;
        Ok(())
    }

    /// Record a blob if it is not yet known.
    pub fn insert_blob(&self, hash: &str, size: u64, created_at: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO blob(hash, size, created_at) VALUES (?1, ?2, ?3)",
            params![hash, u64_to_i64(size), created_at],
        )?;
        Ok(())
    }

    /// Whether a blob is already stored.
    pub fn has_blob(&self, hash: &str) -> Result<bool> {
        let found: Option<i64> = self
            .conn
            .query_row("SELECT 1 FROM blob WHERE hash = ?1", [hash], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(found.is_some())
    }

    /// Record a named entry pointing at an existing blob.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_entry(
        &self,
        blob: &str,
        name: &str,
        path: &str,
        source: &str,
        size: u64,
        created_at: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO entry(blob_hash, name, path, source, size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![blob, name, path, source, u64_to_i64(size), created_at],
        )?;
        let id = self.conn.last_insert_rowid();
        self.fts_upsert(id, name, path, source)?;
        Ok(id)
    }

    /// Fetch a single entry by id.
    pub fn entry(&self, id: i64) -> Result<Option<Entry>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, blob_hash, name, path, source, size, created_at
                 FROM entry WHERE id = ?1",
                [id],
                row_to_entry,
            )
            .optional()?)
    }

    /// Look up entries whose id-like selector matches name, path, source or
    /// the tail of a path (`docs/report.txt` matches `tmp/x/docs/report.txt`).
    pub fn find_entries(&self, needle: &str) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, blob_hash, name, path, source, size, created_at
             FROM entry
             WHERE name = ?1 OR path = ?1 OR source = ?1
                OR path LIKE ?2 OR source LIKE ?2 OR name = ?3
             ORDER BY id",
        )?;
        let suffix = format!("%/{needle}");
        let basename = needle.rsplit('/').next().unwrap_or(needle);
        let rows = stmt.query_map(params![needle, suffix, basename], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// All entries carrying `tag`.
    pub fn entries_with_tag(&self, tag: &str) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.blob_hash, e.name, e.path, e.source, e.size, e.created_at
             FROM entry e JOIN tag t ON t.entry_id = e.id
             WHERE t.tag = ?1 ORDER BY e.path",
        )?;
        let rows = stmt.query_map([tag], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every tag in use, with counts.
    pub fn tags(&self) -> Result<Vec<TagStat>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag, COUNT(*) FROM tag GROUP BY tag ORDER BY tag")?;
        let rows = stmt.query_map([], |row| {
            Ok(TagStat {
                tag: row.get(0)?,
                entries: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Tags attached to one entry.
    pub fn tags_of(&self, entry_id: i64) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag FROM tag WHERE entry_id = ?1 ORDER BY tag")?;
        let rows = stmt.query_map([entry_id], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Attach a tag to an entry. Returns whether it was newly added.
    pub fn add_tag(&self, entry_id: i64, tag: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO tag(entry_id, tag) VALUES (?1, ?2)",
            params![entry_id, tag],
        )?;
        if changed > 0 {
            self.fts_refresh(entry_id)?;
        }
        Ok(changed > 0)
    }

    /// Remove a tag from an entry. Returns whether it existed.
    pub fn remove_tag(&self, entry_id: i64, tag: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "DELETE FROM tag WHERE entry_id = ?1 AND tag = ?2",
            params![entry_id, tag],
        )?;
        if changed > 0 {
            self.fts_refresh(entry_id)?;
        }
        Ok(changed > 0)
    }

    /// Set a metadata key for an entry.
    pub fn set_meta(&self, entry_id: i64, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(entry_id, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(entry_id, key) DO UPDATE SET value = excluded.value",
            params![entry_id, key, value],
        )?;
        Ok(())
    }

    /// Full-text search over names, paths, sources and tags.
    pub fn search(&self, query: &str) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.blob_hash, e.name, e.path, e.source, e.size, e.created_at
             FROM entry_fts f JOIN entry e ON e.id = f.rowid
             WHERE entry_fts MATCH ?1
             ORDER BY bm25(entry_fts), e.path",
        )?;
        let rows = stmt.query_map([query], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Rebuild the search index from scratch.
    pub fn reindex(&self) -> Result<()> {
        self.conn.execute("DELETE FROM entry_fts", [])?;
        let ids: Vec<i64> = {
            let mut stmt = self.conn.prepare("SELECT id FROM entry ORDER BY id")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in ids {
            self.fts_refresh(id)?;
        }
        Ok(())
    }

    /// Write or replace the search document for an entry.
    fn fts_upsert(&self, entry_id: i64, name: &str, path: &str, source: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO entry_fts(rowid, name, path, source, tags) VALUES (?1, ?2, ?3, ?4, '')",
            params![entry_id, name, path, source],
        )?;
        Ok(())
    }

    /// Rebuild the search document for an entry, including its tags.
    fn fts_refresh(&self, entry_id: i64) -> Result<()> {
        let Some(entry) = self.entry(entry_id)? else {
            return Ok(());
        };
        let tags = self.tags_of(entry_id)?.join(" ");
        self.conn
            .execute("DELETE FROM entry_fts WHERE rowid = ?1", [entry_id])?;
        self.conn.execute(
            "INSERT INTO entry_fts(rowid, name, path, source, tags) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![entry_id, entry.name, entry.path, entry.source, tags],
        )?;
        Ok(())
    }

    /// Every entry, newest first.
    pub fn entries(&self) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, blob_hash, name, path, source, size, created_at
             FROM entry ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Entries under a given path prefix.
    pub fn entries_under(&self, prefix: &str) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, blob_hash, name, path, source, size, created_at
             FROM entry WHERE path LIKE ?1 ORDER BY path",
        )?;
        let pattern = format!("{prefix}%");
        let rows = stmt.query_map([pattern], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Entries whose path or source URI starts with `prefix`.
    pub fn entries_matching_prefix(&self, prefix: &str) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, blob_hash, name, path, source, size, created_at
             FROM entry WHERE path LIKE ?1 OR source LIKE ?1 ORDER BY path",
        )?;
        let pattern = format!("%{prefix}%");
        let rows = stmt.query_map([pattern], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Total number of blobs.
    pub fn blob_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM blob", [], |row| row.get(0))?)
    }

    /// Total size of stored blobs.
    pub fn blob_bytes(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(SUM(size), 0) FROM blob", [], |row| {
                row.get(0)
            })?)
    }

    /// Number of entries.
    pub fn entry_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM entry", [], |row| row.get(0))?)
    }

    /// Blobs and their reference counts.
    pub fn blobs(&self) -> Result<Vec<BlobStat>> {
        let mut stmt = self.conn.prepare(
            "SELECT b.hash, b.size, COUNT(e.id)
             FROM blob b LEFT JOIN entry e ON e.blob_hash = b.hash
             GROUP BY b.hash, b.size ORDER BY b.created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(BlobStat {
                hash: row.get(0)?,
                size: i64_to_u64(row.get(1)?),
                entries: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        blob: row.get(1)?,
        name: row.get(2)?,
        path: row.get(3)?,
        source: row.get(4)?,
        size: i64_to_u64(row.get(5)?),
        created_at: row.get(6)?,
    })
}

/// SQLite has no unsigned integer; sizes are stored as `i64`.
fn i64_to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_and_deduplicates_blobs() {
        let index = Index::open_in_memory().unwrap();
        index.insert_blob("abc", 10, 1).unwrap();
        index.insert_blob("abc", 10, 2).unwrap();
        assert!(index.has_blob("abc").unwrap());
        assert!(!index.has_blob("def").unwrap());
        assert_eq!(index.blob_count().unwrap(), 1);
        assert_eq!(index.blob_bytes().unwrap(), 10);
    }

    #[test]
    fn entries_share_a_blob() {
        let index = Index::open_in_memory().unwrap();
        index.insert_blob("abc", 10, 1).unwrap();
        index
            .insert_entry("abc", "a.txt", "a.txt", "mz://local/a.txt", 10, 1)
            .unwrap();
        index
            .insert_entry("abc", "b.txt", "b.txt", "mz://local/b.txt", 10, 1)
            .unwrap();

        assert_eq!(index.entry_count().unwrap(), 2);
        let stats = index.blobs().unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].entries, 2);
    }

    #[test]
    fn entries_under_filters_by_prefix() {
        let index = Index::open_in_memory().unwrap();
        index.insert_blob("abc", 1, 1).unwrap();
        index
            .insert_entry("abc", "x", "logs/a.txt", "s", 1, 1)
            .unwrap();
        index
            .insert_entry("abc", "y", "notes/b.txt", "s", 1, 1)
            .unwrap();

        let found = index.entries_under("logs/").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, "logs/a.txt");
    }

    fn seeded() -> Index {
        let index = Index::open_in_memory().unwrap();
        index.insert_blob("abc", 1, 1).unwrap();
        index
            .insert_entry(
                "abc",
                "report.txt",
                "docs/report.txt",
                "mz://local/docs/report.txt",
                1,
                1,
            )
            .unwrap();
        index
            .insert_entry(
                "abc",
                "photo.jpg",
                "media/photo.jpg",
                "mz://local/media/photo.jpg",
                1,
                1,
            )
            .unwrap();
        index
    }

    #[test]
    fn tags_are_added_removed_and_listed() {
        let index = seeded();
        assert!(index.add_tag(1, "work").unwrap());
        assert!(!index.add_tag(1, "work").unwrap(), "idempotent");
        assert!(index.add_tag(2, "work").unwrap());

        assert_eq!(index.tags_of(1).unwrap(), vec!["work"]);
        let stats = index.tags().unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].entries, 2);

        assert!(index.remove_tag(1, "work").unwrap());
        assert!(!index.remove_tag(1, "work").unwrap());
        assert_eq!(index.entries_with_tag("work").unwrap().len(), 1);
    }

    #[test]
    fn search_finds_names_paths_and_tags() {
        let index = seeded();
        index.add_tag(2, "holiday").unwrap();

        assert_eq!(index.search("report").unwrap().len(), 1);
        assert_eq!(index.search("media").unwrap().len(), 1);
        assert_eq!(index.search("holiday").unwrap().len(), 1);
        assert_eq!(index.search("report media").unwrap().len(), 0);
    }

    #[test]
    fn reindex_restores_searchability() {
        let index = seeded();
        index.add_tag(1, "work").unwrap();
        index.reindex().unwrap();

        assert_eq!(index.search("work").unwrap().len(), 1);
        assert_eq!(index.search("photo").unwrap().len(), 1);
    }

    #[test]
    fn metadata_is_upserted() {
        let index = seeded();
        index.set_meta(1, "author", "ada").unwrap();
        index.set_meta(1, "author", "grace").unwrap();
        let value: String = index
            .conn
            .query_row(
                "SELECT value FROM meta WHERE entry_id = 1 AND key = 'author'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(value, "grace");
    }

    #[test]
    fn find_entries_matches_paths() {
        let index = seeded();
        let found = index.find_entries("docs/report.txt").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "report.txt");
    }

    #[test]
    fn find_entries_matches_basenames_and_tails() {
        let index = seeded();
        assert_eq!(index.find_entries("photo.jpg").unwrap().len(), 1);
        assert_eq!(index.find_entries("docs/report.txt").unwrap().len(), 1);
        assert_eq!(index.find_entries("missing.txt").unwrap().len(), 0);
    }

    #[test]
    fn entries_matching_prefix_checks_sources_too() {
        let index = seeded();
        assert_eq!(index.entries_matching_prefix("docs/").unwrap().len(), 1);
        assert_eq!(index.entries_matching_prefix("media").unwrap().len(), 1);
        assert_eq!(index.entries_matching_prefix("nope").unwrap().len(), 0);
    }
}
