//! Classifying files into media kinds.
//!
//! This lives in the archive layer because thumbnail generation keys off it,
//! and `open` consumes the same classification. It is pure name/byte logic
//! with no I/O.

/// The coarse category of a file, used to pick a handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Video,
    Audio,
    Pdf,
    Other,
}

impl Kind {
    /// Lowercase identifier stored in the handler table.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Audio => "audio",
            Kind::Pdf => "pdf",
            Kind::Other => "other",
        }
    }

    /// Parse a kind given on the command line.
    pub fn parse(raw: &str) -> Option<Kind> {
        Some(match raw.to_ascii_lowercase().as_str() {
            "text" => Kind::Text,
            "image" => Kind::Image,
            "video" => Kind::Video,
            "audio" => Kind::Audio,
            "pdf" => Kind::Pdf,
            "other" => Kind::Other,
            _ => return None,
        })
    }
}

/// Classify a file by its extension alone (no network access).
pub fn kind_of(name: &str) -> Kind {
    let ext = extension_of(name);
    match ext.as_deref() {
        Some(
            ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" | ".bmp" | ".tiff" | ".svg" | ".avif"
            | ".heic",
        ) => Kind::Image,
        Some(".mp4" | ".mkv" | ".mov" | ".webm" | ".avi" | ".m4v" | ".mpg" | ".mpeg") => {
            Kind::Video
        }
        Some(".mp3" | ".flac" | ".wav" | ".ogg" | ".m4a" | ".opus" | ".aac") => Kind::Audio,
        Some(".pdf") => Kind::Pdf,
        Some(
            ".txt" | ".md" | ".markdown" | ".rst" | ".log" | ".json" | ".toml" | ".yaml" | ".yml"
            | ".csv" | ".tsv" | ".ini" | ".conf" | ".cfg" | ".sh" | ".bash" | ".zsh" | ".fish"
            | ".py" | ".rs" | ".go" | ".js" | ".ts" | ".c" | ".h" | ".cpp" | ".hpp" | ".java"
            | ".rb" | ".php" | ".sql" | ".html" | ".css" | ".xml",
        ) => Kind::Text,
        _ => Kind::Other,
    }
}

/// Refine a kind using the leading bytes of the file.
///
/// Only called when the extension was inconclusive, so ordinary files pay
/// nothing. This is what removes most need for `--as`.
pub fn sniff_kind(leading: &[u8]) -> Kind {
    if leading.starts_with(&[0x89, b'P', b'N', b'G'])
        || leading.starts_with(&[0xff, 0xd8, 0xff])
        || leading.starts_with(b"GIF8")
    {
        return Kind::Image;
    }
    if leading.len() >= 12 && &leading[0..4] == b"RIFF" && &leading[8..12] == b"WEBP" {
        return Kind::Image;
    }
    if leading.starts_with(b"%PDF-") {
        return Kind::Pdf;
    }
    if leading.len() >= 12 && &leading[4..8] == b"ftyp" {
        return Kind::Video;
    }
    Kind::Other
}

/// Resolve a kind from the name, sniffing the head when that is unclear.
pub fn classify(name: &str, leading: Option<&[u8]>) -> Kind {
    let by_name = kind_of(name);
    if by_name != Kind::Other {
        return by_name;
    }
    match leading {
        Some(bytes) => sniff_kind(bytes),
        None => Kind::Other,
    }
}

/// The lowercased extension including the dot, if any.
pub fn extension_of(name: &str) -> Option<String> {
    let base = name.rsplit('/').next().unwrap_or(name);
    let (_, ext) = base.rsplit_once('.')?;
    if ext.is_empty() {
        return None;
    }
    Some(format!(".{}", ext.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_extension() {
        assert_eq!(kind_of("photo.jpg"), Kind::Image);
        assert_eq!(kind_of("clip.MKV"), Kind::Video);
        assert_eq!(kind_of("song.flac"), Kind::Audio);
        assert_eq!(kind_of("doc.pdf"), Kind::Pdf);
        assert_eq!(kind_of("notes.md"), Kind::Text);
        assert_eq!(kind_of("archive.tar"), Kind::Other);
        assert_eq!(kind_of("Makefile"), Kind::Other);
    }

    #[test]
    fn extracts_extensions() {
        assert_eq!(extension_of("a/b/Photo.PNG").as_deref(), Some(".png"));
        assert_eq!(extension_of("noext"), None);
        assert_eq!(extension_of("trailing."), None);
    }

    #[test]
    fn sniffs_common_media_signatures() {
        assert_eq!(sniff_kind(&[0x89, b'P', b'N', b'G', b'\r']), Kind::Image);
        assert_eq!(sniff_kind(&[0xff, 0xd8, 0xff, 0xe0]), Kind::Image);
        assert_eq!(sniff_kind(b"GIF89a....."), Kind::Image);
        assert_eq!(sniff_kind(b"%PDF-1.7"), Kind::Pdf);

        let mut mp4 = b"....ftypisom".to_vec();
        mp4[0] = 0;
        assert_eq!(sniff_kind(&mp4), Kind::Video);

        assert_eq!(sniff_kind(b"plain text here"), Kind::Other);
    }

    #[test]
    fn classify_prefers_the_extension_then_sniffs() {
        // A known extension wins without any bytes.
        assert_eq!(classify("photo.jpg", None), Kind::Image);
        // An unknown extension falls back to the leading bytes.
        assert_eq!(
            classify("mystery", Some(&[0x89, b'P', b'N', b'G'])),
            Kind::Image
        );
        assert_eq!(classify("mystery", None), Kind::Other);
    }

    #[test]
    fn kind_round_trips() {
        assert_eq!(Kind::parse("image"), Some(Kind::Image));
        assert_eq!(Kind::parse("VIDEO"), Some(Kind::Video));
        assert_eq!(Kind::parse("nope"), None);
        assert_eq!(Kind::Image.as_str(), "image");
    }
}
