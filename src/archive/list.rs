//! Rendering archived entries.

use serde::Serialize;

use crate::archive::db::Entry;
use crate::output::human_size;

/// A serializable view of an archived entry.
#[derive(Debug, Clone, Serialize)]
pub struct EntryRecord {
    pub id: i64,
    pub hash: String,
    pub backend: String,
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
            backend: backend_of(&entry.source),
            name: entry.name.clone(),
            path: entry.path.clone(),
            source: entry.source.clone(),
            size: entry.size,
        }
    }
}

impl EntryRecord {
    /// A single line for `search` / `archive`: backend, path, size, hash.
    pub fn line(&self) -> String {
        let short = self.hash.get(0..8).unwrap_or(&self.hash);
        format!(
            "{:<8} {size:>8}  {:<8} {}",
            self.backend,
            short,
            self.path,
            size = human_size(self.size)
        )
    }

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

/// Extract the backend name from an `mz://<backend>/...` source URI.
fn backend_of(source: &str) -> String {
    source
        .strip_prefix("mz://")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("")
        .to_string()
}
