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
        Ok(self.conn.last_insert_rowid())
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
}
