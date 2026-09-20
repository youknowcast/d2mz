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

use crate::backend::Backends;
use crate::cli::OpenArgs;
use crate::uri::Uri;
use d2mz_archive::Archive;
use d2mz_archive::media::{Kind, classify, extension_of, kind_of};

pub async fn run(backends: &mut Backends, archive: &Archive, args: OpenArgs) -> Result<()> {
    let uri = Uri::parse(&args.uri)?;
    let operator = backends.resolve(&uri)?;

    let metadata = operator.stat(uri.path()).await;

    // A vanished source falls back to the archived blob, which is precisely
    // what the archive is for.
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == opendal::ErrorKind::NotFound => {
            if let Some((_, path)) = archive.blob_for_source(&uri.to_string())? {
                return open_archived(archive, &uri, &path, &args);
            }
            return Err(error).with_context(|| format!("stat {uri}"));
        }
        Err(error) => return Err(error).with_context(|| format!("stat {uri}")),
    };

    if metadata.is_dir() {
        // Directories are listed rather than "opened".
        let view = crate::output::EntryView::from_metadata(uri.backend(), uri.path(), &metadata);
        println!("{}", view.long());
        return Ok(());
    }

    let name = uri.path().rsplit('/').next().unwrap_or(uri.path());
    let kind = match args.as_kind.as_deref() {
        Some(raw) => Kind::parse(raw).with_context(|| format!("unknown kind {raw:?}"))?,
        None => {
            // Only peek at the bytes when the extension said nothing.
            let leading = peek(backends, &uri).await;
            classify(name, leading.as_deref())
        }
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

/// Open a blob that already lives in the archive (source is gone).
fn open_archived(
    archive: &Archive,
    uri: &Uri,
    blob: &std::path::Path,
    args: &OpenArgs,
) -> Result<()> {
    let name = uri.path().rsplit('/').next().unwrap_or(uri.path());
    let kind = match args.as_kind.as_deref() {
        Some(raw) => Kind::parse(raw).with_context(|| format!("unknown kind {raw:?}"))?,
        None => {
            let leading = read_leading(blob);
            classify(name, leading.as_deref())
        }
    };
    let extension = extension_of(name);

    if args.print {
        println!("{}", blob.display());
        return Ok(());
    }

    let command = resolve_command(archive, kind, &extension, args.with.as_deref())?;
    launch(&command, blob)
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
        "no application for kind {}; set one with \
         `d2mz handler set {} --app '<command> {{}}'`",
        kind.as_str(),
        kind.as_str()
    )
}

/// Built-in opener for a kind, if the platform has an obvious one.
fn builtin(kind: Kind) -> Option<String> {
    match kind {
        Kind::Text => Some(format!("{} {{}}", pager())),
        _ => os_opener()
            // Only offer the OS opener when it actually exists, so the
            // failure message can point at handlers instead.
            .filter(|opener| on_path(opener))
            .map(|opener| format!("{opener} {{}}")),
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

/// Read the leading bytes of an object without materialising it.
///
/// Best-effort: an error just means no sniffing happens.
async fn peek(backends: &mut Backends, uri: &Uri) -> Option<Vec<u8>> {
    if kind_of(uri.path()) != Kind::Other {
        // The extension already answered; do not read anything.
        return None;
    }
    let operator = backends.resolve(uri).ok()?;
    let buffer = operator.read_with(uri.path()).range(0..16).await.ok()?;
    Some(buffer.to_vec())
}

/// Read the leading bytes of a local file, best-effort.
fn read_leading(path: &std::path::Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buffer = [0u8; 16];
    let read = file.read(&mut buffer).ok()?;
    Some(buffer[..read].to_vec())
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

    // Say what is about to run, so a handler taking effect is visible.
    eprintln!("opening with: {command}");

    if !on_path(&program) {
        bail!(
            "{program} is not on PATH; set a handler with \
             `d2mz handler set <kind> --app '<command> {{}}'` or use `--with`"
        );
    }

    let status = Command::new(&program)
        .args(&args)
        .status()
        .with_context(|| format!("running {program}"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}

/// Whether `program` can be found on `PATH` (or is an explicit path).
fn on_path(program: &str) -> bool {
    if program.contains('/') {
        return std::path::Path::new(program).is_file();
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file()
    })
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
    fn splits_commands_with_quotes() {
        let (program, args) = split_command("imv -q '{}'");
        assert_eq!(program, "imv");
        assert_eq!(args, vec!["-q", "{}"]);

        let (program, args) = split_command("xdg-open");
        assert_eq!(program, "xdg-open");
        assert!(args.is_empty());
    }
}
