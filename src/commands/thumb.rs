//! The `thumb` command: generate, cache and display thumbnails.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::archive::Archive;
use crate::archive::thumb::{ensure, thumb_path};
use crate::backend::Backends;
use crate::cli::ThumbArgs;
use crate::commands::open::{Kind, kind_of};
use crate::uri::Uri;

/// Viewers that can render an image into the terminal, in preference order.
const VIEWERS: [&str; 4] = ["chafa", "viu", "tiv", "img2txt"];

pub async fn run(backends: &mut Backends, archive: &Archive, args: ThumbArgs) -> Result<()> {
    let uri = Uri::parse(&args.uri)?;

    // The thumbnail is keyed by content hash, so the object must be archived.
    let hash = hash_for(archive, &uri)?;
    let name = uri.path().rsplit('/').next().unwrap_or(uri.path());
    let kind = kind_of(name);

    if args.print {
        // Show the path only, generating on first use.
        if let Some(thumb) = ensure(archive, &hash, kind)? {
            println!("{}", thumb.path.display());
        } else {
            // Not thumbnailable: fall back to the blob itself.
            println!("{}", archive.blob_path(&hash).display());
        }
        return Ok(());
    }

    let Some(thumb) = ensure(archive, &hash, kind)? else {
        bail!("{uri} has no thumbnail ({kind:?}); use `d2mz open` instead");
    };

    display(&thumb.path)?;
    if args.long {
        eprintln!(
            "{}x{} {} ({})",
            thumb.width,
            thumb.height,
            thumb.format,
            crate::output::human_size(thumb.size)
        );
    }
    let _ = backends;
    Ok(())
}

/// The blob hash for a URI, looking it up in the index.
fn hash_for(archive: &Archive, uri: &Uri) -> Result<String> {
    let entry = archive
        .index()
        .entry_by_source(&uri.to_string())?
        .or_else(|| {
            archive
                .index()
                .find_entries(uri.path())
                .ok()
                .and_then(|mut e| e.pop())
        })
        .with_context(|| format!("{uri} is not archived; run `d2mz ingest {uri}` first"))?;
    Ok(entry.blob)
}

/// Render a thumbnail into the terminal with the best available viewer.
fn display(path: &PathBuf) -> Result<()> {
    if let Some(viewer) = find_viewer() {
        let status = Command::new(&viewer)
            .arg(path)
            .status()
            .with_context(|| format!("running {viewer}"))?;
        if !status.success() {
            bail!("{viewer} exited with {status}");
        }
        return Ok(());
    }

    // No terminal image viewer: describe the thumbnail instead of dumping
    // binary into the terminal.
    eprintln!(
        "no terminal image viewer found (tried {}); thumbnail at {}",
        VIEWERS.join(", "),
        path.display()
    );
    Ok(())
}

/// The first available terminal image viewer, if any.
fn find_viewer() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for viewer in VIEWERS {
        if std::env::split_paths(&path).any(|dir| dir.join(viewer).is_file()) {
            return Some(viewer.to_string());
        }
    }
    None
}

/// Where a blob's thumbnail is stored, convenient for tests.
pub fn path_of(archive: &Archive, hash: &str) -> PathBuf {
    thumb_path(archive, hash)
}

/// Kinds that support thumbnails.
pub fn supports(kind: Kind) -> bool {
    matches!(kind, Kind::Image | Kind::Video)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_media_kinds_support_thumbnails() {
        assert!(supports(Kind::Image));
        assert!(supports(Kind::Video));
        assert!(!supports(Kind::Text));
        assert!(!supports(Kind::Pdf));
    }
}
