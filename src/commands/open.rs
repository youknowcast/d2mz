//! Opening an object with an application chosen by kind.
//!
//! Resolution order for the command to run:
//!
//! 1. `--with`
//! 2. a handler stored in the index (extension-specific, then kind `*`)
//! 3. a built-in default (pager for text, the OS opener otherwise)
//!
//! Remote objects are materialised into a temporary file first; text is
//! streamed straight to the pager so it is never copied.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::archive::Archive;
use crate::backend::Backends;
use crate::cli::OpenArgs;
use crate::uri::Uri;

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

/// The lowercased extension including the dot, if any.
pub fn extension_of(name: &str) -> Option<String> {
    let base = name.rsplit('/').next().unwrap_or(name);
    let (_, ext) = base.rsplit_once('.')?;
    if ext.is_empty() {
        return None;
    }
    Some(format!(".{}", ext.to_ascii_lowercase()))
}

pub async fn run(backends: &mut Backends, archive: &Archive, args: OpenArgs) -> Result<()> {
    let uri = Uri::parse(&args.uri)?;
    let operator = backends.resolve(&uri)?;

    let metadata = operator
        .stat(uri.path())
        .await
        .with_context(|| format!("stat {uri}"))?;

    if metadata.is_dir() {
        // Directories are listed rather than "opened".
        let view = crate::output::EntryView::from_metadata(uri.backend(), uri.path(), &metadata);
        println!("{}", view.long());
        return Ok(());
    }

    let name = uri.path().rsplit('/').next().unwrap_or(uri.path());
    let kind = match args.as_kind.as_deref() {
        Some(raw) => Kind::parse(raw).with_context(|| format!("unknown kind {raw:?}"))?,
        None => kind_of(name),
    };
    let extension = extension_of(name);

    // `--print` never opens anything: it reports the local path that would be
    // used, materialising a remote object so the path is real.
    if args.print {
        let path = materialise(backends, &uri).await?;
        println!("{}", path.display());
        return Ok(());
    }

    // Text goes to the pager without touching disk when it is local.
    if kind == Kind::Text
        && args.with.is_none()
        && stored_handler(archive, kind, &extension)?.is_none()
    {
        return page(backends, &uri, archive).await;
    }

    let command = resolve_command(archive, kind, &extension, args.with.as_deref())?;
    let path = materialise(backends, &uri).await?;
    launch(&command, &path)
}

/// The handler stored in the index, if any.
fn stored_handler(
    archive: &Archive,
    kind: Kind,
    extension: &Option<String>,
) -> Result<Option<String>> {
    Ok(archive
        .index()
        .resolve_handler(kind.as_str(), extension.as_deref())?
        .map(|handler| handler.app))
}

/// Decide which command to run.
fn resolve_command(
    archive: &Archive,
    kind: Kind,
    extension: &Option<String>,
    explicit: Option<&str>,
) -> Result<String> {
    if let Some(command) = explicit {
        return Ok(command.to_string());
    }
    if let Some(stored) = stored_handler(archive, kind, extension)? {
        return Ok(stored);
    }
    if let Some(default) = builtin(kind) {
        return Ok(default);
    }
    bail!(
        "no application for {kind:?}; set one with `d2mz handler set {} --app '...'`",
        kind.as_str()
    )
}

/// Built-in opener for a kind, if the platform has an obvious one.
fn builtin(kind: Kind) -> Option<String> {
    match kind {
        Kind::Text => Some(format!("{} {{}}", pager())),
        _ => os_opener().map(|opener| format!("{opener} {{}}")),
    }
}

/// The pager to use for text output.
pub fn pager() -> String {
    std::env::var("PAGER").unwrap_or_else(|_| "less".to_string())
}

/// The platform's default "open with the associated app" command.
pub fn os_opener() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("open")
    } else if cfg!(target_os = "windows") {
        Some("start")
    } else {
        Some("xdg-open")
    }
}

/// Stream a text object to the pager.
async fn page(backends: &mut Backends, uri: &Uri, _archive: &Archive) -> Result<()> {
    let pager = pager();
    let operator = backends.resolve(uri)?;

    if uri.backend() == "local" {
        // Local files can be handed to the pager directly.
        let path = PathBuf::from("/").join(uri.path());
        return launch(&format!("{pager} {{}}"), &path);
    }

    // Remote text: read it fully and pipe it to the pager's stdin. Text is
    // small; this avoids writing a temporary file.
    let bytes = operator
        .read(uri.path())
        .await
        .with_context(|| format!("reading {uri}"))?
        .to_vec();
    let mut child = Command::new(&pager)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning pager {pager}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(&bytes)?;
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("pager {pager} exited with {status}");
    }
    Ok(())
}

/// Ensure the object exists locally and return its path.
async fn materialise(backends: &mut Backends, uri: &Uri) -> Result<PathBuf> {
    if uri.backend() == "local" {
        return Ok(PathBuf::from("/").join(uri.path()));
    }

    let operator = backends.resolve(uri)?;
    let bytes = operator
        .read(uri.path())
        .await
        .with_context(|| format!("reading {uri}"))?
        .to_vec();

    let name = uri.path().rsplit('/').next().unwrap_or("object");
    let dir = std::env::temp_dir().join("d2mz-open");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}-{name}", std::process::id()));
    std::fs::write(&path, &bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Run a handler command, substituting `{}` with the path.
fn launch(command: &str, path: &std::path::Path) -> Result<()> {
    let path = path.to_string_lossy();
    let (program, args) = if command.contains("{}") {
        let expanded = command.replace("{}", &path);
        split_command(&expanded)
    } else {
        split_command(command)
    };
    let status = Command::new(&program)
        .args(&args)
        .status()
        .with_context(|| format!("running {program}"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}

/// Split a command line into program and arguments, honouring simple quotes.
fn split_command(command: &str) -> (String, Vec<String>) {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in command.chars() {
        match ch {
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            c if c.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    let program = parts.first().cloned().unwrap_or_default();
    (program, parts.into_iter().skip(1).collect())
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
    fn kind_round_trips() {
        assert_eq!(Kind::parse("image"), Some(Kind::Image));
        assert_eq!(Kind::parse("VIDEO"), Some(Kind::Video));
        assert_eq!(Kind::parse("nope"), None);
        assert_eq!(Kind::Image.as_str(), "image");
    }

    #[test]
    fn splits_commands_with_quotes() {
        let (program, args) = split_command("imv -q '{}'");
        assert_eq!(program, "imv");
        assert_eq!(args, vec!["-q", "{}"]);

        let (program, args) = split_command("xdg-open");
        assert_eq!(program, "xdg-open");
        assert!(args.is_empty());
    }
}
