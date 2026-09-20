//! Rendering archived entries.

use serde::Serialize;

use crate::archive::db::Entry;
use crate::output::human_size;

/// A serializable view of an archived entry.
#[derive(Debug, Clone, Serialize)]
pub struct EntryRecord {
    pub id: i64,
    pub hash: String,
    pub name: String,
    pub path: String,
    pub source: String,
    pub size: u64,
}

impl From<&Entry> for EntryRecord {
    fn from(entry: &Entry) -> EntryRecord {
        EntryRecord {
            id: entry.id,
            hash: entry.blob.clone(),
            name: entry.name.clone(),
            path: entry.path.clone(),
            source: entry.source.clone(),
            size: entry.size,
        }
    }
}

impl EntryRecord {
    /// A single `ls -l`-style line.
    pub fn long(&self) -> String {
        let short = self.hash.get(0..8).unwrap_or(&self.hash);
        format!(
            "{short} {size:>8}  {}",
            self.source,
            size = human_size(self.size)
        )
    }
}
