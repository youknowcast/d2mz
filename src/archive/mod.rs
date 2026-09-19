//! The content-addressed archive.
//!
//! An archive is two things: a directory of blobs addressed by their BLAKE3
//! hash, and a SQLite index describing the named entries that point at them.
//! A blob is stored once no matter how many entries share its contents.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub mod db;
pub mod ingest;
pub mod list;
pub mod lock;
pub mod sync;
pub mod thumb;

use db::Index;

/// Handle to an on-disk archive.
pub struct Archive {
    root: PathBuf,
    index: Index,
}

impl Archive {
    /// Open (or create) the archive rooted at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Archive> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("store"))
            .with_context(|| format!("creating archive store at {}", root.display()))?;
        let index = Index::open(&root.join("index.sqlite3"))?;
        // Best-effort housekeeping: drop temp files from a crashed ingest.
        sweep_temp_files(&root);
        Ok(Archive { root, index })
    }

    /// Root directory of the archive.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The underlying index.
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Path of the blob with the given hash.
    pub fn blob_path(&self, hash: &str) -> PathBuf {
        self.root.join("store").join(blob_relative_path(hash))
    }

    /// Read a blob by hash.
    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        fs::read(self.blob_path(hash)).with_context(|| format!("reading blob {hash}"))
    }

    /// Whether a blob is both indexed and present on disk.
    pub fn has_blob_on_disk(&self, hash: &str) -> bool {
        self.blob_path(hash).exists()
    }
}

/// Shard a hash into `aa/bb/<hash>` to avoid one enormous directory.
pub fn blob_relative_path(hash: &str) -> PathBuf {
    let (a, b) = (
        hash.get(0..2).unwrap_or("00"),
        hash.get(2..4).unwrap_or("00"),
    );
    Path::new(a).join(b).join(hash)
}

/// Remove leftover `.tmp.*` files from interrupted ingests.
///
/// Ingest writes to `store/.tmp.<pid>.<n>` and renames it into place; a
/// crash can leave one behind. They are never referenced, so clearing them
/// is always safe.
fn sweep_temp_files(root: &Path) {
    let store = root.join("store");
    let Ok(entries) = fs::read_dir(&store) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(".tmp.") {
            let _ = fs::remove_file(entry.path());
        }
    }
}
