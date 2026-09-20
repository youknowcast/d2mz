//! Views of archived entries.
//!
//! `EntryRecord` is a plain data view of an index entry; how it is rendered
//! is the CLI's concern.

use serde::Serialize;

use crate::db::Entry;

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
    pub state: String,
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
            state: entry.state.as_str().to_string(),
        }
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
