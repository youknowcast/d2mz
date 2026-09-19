//! Thumbnail generation and caching.
//!
//! Thumbnails are keyed by the source blob's BLAKE3 hash, so identical
//! contents share one thumbnail and regenerating is a no-op. Images are
//! decoded and resized in-process with the pure-Rust `image` crate; video
//! frames are extracted with `ffmpeg` when it is installed.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::archive::Archive;
use crate::commands::open::Kind;

/// Longest edge of a generated thumbnail, in pixels.
pub const THUMB_EDGE: u32 = 256;

/// A generated thumbnail on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedThumb {
    /// Encoded width.
    pub width: u32,
    /// Encoded height.
    pub height: u32,
    /// Storage format, always `webp` today.
    pub format: &'static str,
    /// Encoded size in bytes.
    pub size: u64,
    /// Path the thumbnail was written to.
    pub path: PathBuf,
}

/// Where a blob's thumbnail lives.
pub fn thumb_path(archive: &Archive, hash: &str) -> PathBuf {
    archive
        .root()
        .join("thumb")
        .join(crate::archive::blob_relative_path(hash))
        .with_extension("webp")
}

/// Generate a thumbnail for `blob_hash` from its stored bytes, if needed.
///
/// Returns `Ok(None)` for kinds that have no thumbnail support.
pub fn ensure(archive: &Archive, blob_hash: &str, kind: Kind) -> Result<Option<GeneratedThumb>> {
    if !matches!(kind, Kind::Image | Kind::Video) {
        return Ok(None);
    }
    if let Some(existing) = archive.index().thumb(blob_hash)? {
        let path = thumb_path(archive, blob_hash);
        if path.exists() {
            return Ok(Some(GeneratedThumb {
                width: existing.width,
                height: existing.height,
                format: "webp",
                size: existing.size,
                path,
            }));
        }
    }

    let source = archive.read_blob(blob_hash)?;
    let generated = match kind {
        Kind::Image => from_image(&source, &thumb_path(archive, blob_hash))?,
        Kind::Video => match from_video(&source, &thumb_path(archive, blob_hash))? {
            Some(thumb) => thumb,
            None => return Ok(None),
        },
        _ => return Ok(None),
    };

    archive.index().set_thumb(
        blob_hash,
        generated.width,
        generated.height,
        generated.format,
        generated.size,
    )?;
    Ok(Some(generated))
}

/// Decode an image and write a resized WebP thumbnail.
fn from_image(bytes: &[u8], out: &Path) -> Result<GeneratedThumb> {
    let image = image::load_from_memory(bytes).context("decoding image")?;
    let thumb = image.thumbnail(THUMB_EDGE, THUMB_EDGE);
    write_webp(&thumb, out)
}

/// Extract the first frame of a video with ffmpeg, if available.
fn from_video(bytes: &[u8], out: &Path) -> Result<Option<GeneratedThumb>> {
    if which("ffmpeg").is_none() {
        return Ok(None);
    }

    let dir = std::env::temp_dir().join("d2mz-thumb");
    std::fs::create_dir_all(&dir)?;
    let input = dir.join(format!("{}.input", std::process::id()));
    std::fs::write(&input, bytes).with_context(|| format!("writing {}", input.display()))?;

    let scale = format!("scale={THUMB_EDGE}:{THUMB_EDGE}:force_original_aspect_ratio=decrease");
    let status = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-y", "-i"])
        .arg(&input)
        .args(["-frames:v", "1", "-vf", &scale])
        .arg(out)
        .status()
        .context("running ffmpeg")?;
    let _ = std::fs::remove_file(&input);
    if !status.success() {
        bail!("ffmpeg failed to extract a frame");
    }

    let image = image::open(out).context("decoding extracted frame")?;
    write_webp(&image, out).map(Some)
}

/// Encode `image` as WebP at `out` and describe it.
fn write_webp(image: &image::DynamicImage, out: &Path) -> Result<GeneratedThumb> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    image
        .save_with_format(out, image::ImageFormat::WebP)
        .with_context(|| format!("writing {}", out.display()))?;
    let size = std::fs::metadata(out)?.len();
    Ok(GeneratedThumb {
        width: image.width(),
        height: image.height(),
        format: "webp",
        size,
        path: out.to_path_buf(),
    })
}

/// Whether a program is on `PATH`.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let mut buffer = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }

    #[test]
    fn images_get_a_resized_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("thumb.webp");
        let generated = from_image(&png_bytes(1024, 512), &out).unwrap();
        assert!(generated.path.exists());
        assert!(generated.width <= THUMB_EDGE);
        assert!(generated.height <= THUMB_EDGE);
        assert_eq!(generated.format, "webp");
    }

    #[test]
    fn video_without_ffmpeg_is_skipped() {
        if which("ffmpeg").is_some() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("frame.webp");
        assert!(from_video(b"not a video", &out).unwrap().is_none());
    }
}
