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

impl EntryRecord {
    /// A single line for `search` / `archive`: state marker, backend, path,
    /// size and hash prefix.
    pub fn line(&self) -> String {
        use crate::output::pad_display;

        let short = self.hash.get(0..8).unwrap_or(&self.hash);
        let marker = match self.state.as_str() {
            "missing" => "!",
            "deleted" => "x",
            _ => " ",
        };
        // Backend is ASCII, but paths may be CJK and must be padded by
        // display width, not byte length.
        let size = human_size(self.size);
        format!(
            "{marker}{} {size:>8}  {short:<8} {}",
            pad_display(&self.backend, 8),
            self.path,
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
