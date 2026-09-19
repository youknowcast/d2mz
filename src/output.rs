//! Presentation helpers shared by the browse commands.

use opendal::Entry;
use serde::Serialize;

/// A backend-neutral view of one listed entry.
#[derive(Debug, Clone, Serialize)]
pub struct EntryView {
    /// Fully qualified `mz://` URI.
    pub uri: String,
    /// Final path segment.
    pub name: String,
    /// Backend-relative path.
    pub path: String,
    /// `"dir"` or `"file"`.
    pub kind: &'static str,
    /// Size in bytes; `null` for directories.
    pub size: Option<u64>,
    /// Last modification time as RFC 3339, when known.
    pub modified: Option<String>,
}

impl EntryView {
    /// Project an OpenDAL entry into a display view.
    pub fn from_entry(backend: &str, entry: &Entry) -> EntryView {
        let metadata = entry.metadata();
        let is_dir = metadata.is_dir();
        let path = entry.path().trim_end_matches('/').to_string();
        EntryView {
            uri: format!("mz://{backend}/{path}"),
            name: entry.name().trim_end_matches('/').to_string(),
            path,
            kind: if is_dir { "dir" } else { "file" },
            size: (!is_dir).then(|| metadata.content_length()),
            modified: metadata
                .last_modified()
                .map(|ts| ts.into_inner().to_string()),
        }
    }

    /// Name with a trailing slash for directories.
    pub fn plain(&self) -> String {
        if self.kind == "dir" {
            format!("{}/", self.name)
        } else {
            self.name.clone()
        }
    }

    /// A single `ls -l`-style line.
    pub fn long(&self) -> String {
        let kind = if self.kind == "dir" { "d" } else { "-" };
        let size = match self.size {
            Some(bytes) => human_size(bytes),
            None => "-".to_string(),
        };
        let modified = self.modified.as_deref().unwrap_or("-");
        format!(
            "{kind} {size:>8} {modified:<20} {}",
            self.plain()
        )
    }
}

/// Render a byte count using binary-ish short units.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_units() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(512), "512B");
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(1024 * 1024), "1.0M");
    }
}
