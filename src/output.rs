//! Presentation helpers shared by the browse commands.

use opendal::{Entry, Metadata};
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

impl<'de> serde::Deserialize<'de> for EntryView {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            uri: String,
            name: String,
            path: String,
            kind: String,
            size: Option<u64>,
            modified: Option<String>,
        }

        let raw = Raw::deserialize(deserializer)?;
        let kind = match raw.kind.as_str() {
            "dir" => "dir",
            "file" => "file",
            other => return Err(serde::de::Error::custom(format!("unknown kind {other:?}"))),
        };
        Ok(EntryView {
            uri: raw.uri,
            name: raw.name,
            path: raw.path,
            kind,
            size: raw.size,
            modified: raw.modified,
        })
    }
}

impl EntryView {
    /// Project an OpenDAL entry into a display view.
    pub fn from_entry(backend: &str, entry: &Entry) -> EntryView {
        EntryView::from_metadata(backend, entry.path(), entry.metadata())
    }

    /// Project a path plus metadata into a display view.
    pub fn from_metadata(backend: &str, path: &str, metadata: &Metadata) -> EntryView {
        let is_dir = metadata.is_dir();
        let path = path.trim_end_matches('/').to_string();
        let name = path
            .rsplit_once('/')
            .map(|(_, name)| name)
            .unwrap_or(&path)
            .to_string();
        EntryView {
            uri: format!("mz://{backend}/{path}"),
            name,
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
        format!("{kind} {size:>8} {modified:<20} {}", self.plain())
    }
}

/// Detailed metadata for a single object, used by `stat`.
#[derive(Debug, Clone, Serialize)]
pub struct ObjectView {
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
    /// MIME type, when advertised by the backend.
    pub content_type: Option<String>,
    /// ETag, when advertised by the backend.
    pub etag: Option<String>,
}

impl ObjectView {
    /// Project a path plus metadata into an object view.
    pub fn from_metadata(backend: &str, path: &str, metadata: &Metadata) -> ObjectView {
        let base = EntryView::from_metadata(backend, path, metadata);
        ObjectView {
            uri: base.uri,
            name: base.name,
            path: base.path,
            kind: base.kind,
            size: base.size,
            modified: base.modified,
            content_type: metadata.content_type().map(str::to_string),
            etag: metadata.etag().map(str::to_string),
        }
    }

    /// Multi-line human-readable rendering.
    pub fn verbose(&self) -> String {
        let size = self.size.map(human_size).unwrap_or_else(|| "-".to_string());
        let mut lines = vec![
            format!("uri:          {}", self.uri),
            format!("type:         {}", self.kind),
            format!("size:         {size}"),
            format!("modified:     {}", self.modified.as_deref().unwrap_or("-")),
        ];
        if let Some(content_type) = &self.content_type {
            lines.push(format!("content-type: {content_type}"));
        }
        if let Some(etag) = &self.etag {
            lines.push(format!("etag:         {etag}"));
        }
        lines.join("\n")
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
