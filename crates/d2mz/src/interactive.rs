//! Interactive selection through `fzf`.
//!
//! When the output is a terminal and `fzf` is installed, list commands let
//! the user narrow the results interactively instead of printing them. The
//! chosen action comes from the key pressed:
//!
//! - `Enter` opens the selection (`d2mz open`)
//! - `Ctrl-P` prints the selection's URI to stdout
//!
//! (`Ctrl-Enter` is not a key fzf can report, so `Ctrl-P` and `Alt-Enter`
//! are used instead.)
//!
//! Everything falls back to plain output when fzf is missing or the output
//! is not a terminal, so piping and `--json` stay script-friendly.

use std::io::{IsTerminal, Write};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// One selectable row: the URI to act on and how it should read on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The URI handed to `open` or printed.
    pub uri: String,
    /// The human-readable line shown in fzf.
    pub display: String,
}

impl Candidate {
    /// A candidate from a URI and its display line.
    pub fn new(uri: impl Into<String>, display: impl Into<String>) -> Candidate {
        Candidate {
            uri: uri.into(),
            display: display.into(),
        }
    }
}

/// What the user asked for by pressing a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Open the selection.
    Open,
    /// Print the selection's URI.
    Print,
}

/// The default action when `--print`/`--open` are not given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultAction {
    /// `Enter` opens, the print key prints.
    Open,
    /// `Enter` prints, the print key opens.
    Print,
}

/// The key fzf reports when the user wants to print instead of open.
///
/// `ctrl-enter` is not reportable by fzf, so `ctrl-p` is the primary key and
/// `alt-enter` an alias.
const PRINT_KEYS: &str = "ctrl-p,alt-enter";

/// Whether interactive selection is possible: a terminal and `fzf`.
pub fn available() -> bool {
    std::io::stdout().is_terminal() && fzf_on_path()
}

/// Whether the standard output is a terminal.
pub fn is_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Run fzf over `candidates`, returning the chosen URI and the action.
///
/// Returns `Ok(None)` when the user aborts (Esc or no candidates).
pub fn pick(candidates: &[Candidate], default: DefaultAction) -> Result<Option<(String, Action)>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    if !fzf_on_path() {
        bail!("fzf is not installed; run without interactive selection");
    }

    let mut child = Command::new("fzf")
        .arg(format!("--expect={PRINT_KEYS}"))
        .args([
            "--delimiter=\t",
            "--with-nth=2..",
            "--no-multi",
            "--header=Enter: open   Ctrl-P: print",
        ])
        // fzf prints the pressed key as the first line (empty for Enter).
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("spawning fzf")?;

    {
        let stdin = child.stdin.take().context("fzf stdin")?;
        let mut stdin = std::io::BufWriter::new(stdin);
        for candidate in candidates {
            // The URI must not contain tabs; display is free-form.
            let uri = candidate.uri.replace('\t', " ");
            writeln!(stdin, "{uri}\t{}", candidate.display)?;
        }
        stdin.flush()?;
    }

    let output = child.wait_with_output().context("waiting for fzf")?;
    if !output.status.success() {
        // Esc / no match: not an error, just nothing chosen.
        return Ok(None);
    }

    let text = String::from_utf8(output.stdout).context("fzf output was not UTF-8")?;
    Ok(parse_choice(&text, default))
}

/// Parse fzf's `--expect` output: a key line, then the chosen row.
///
/// Exposed for testing, since driving an interactive fzf from a test is not
/// practical.
pub fn parse_choice(text: &str, default: DefaultAction) -> Option<(String, Action)> {
    let mut lines = text.lines();
    let key = lines.next().unwrap_or("");
    let row = lines.next()?;
    let uri = row.split('\t').next().unwrap_or(row).to_string();
    if uri.is_empty() {
        return None;
    }

    let action = match (key, default) {
        ("ctrl-p" | "alt-enter", DefaultAction::Open) => Action::Print,
        ("ctrl-p" | "alt-enter", DefaultAction::Print) => Action::Open,
        (_, DefaultAction::Open) => Action::Open,
        (_, DefaultAction::Print) => Action::Print,
    };
    Some((uri, action))
}

/// Whether `fzf` is on `PATH`.
fn fzf_on_path() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join("fzf").is_file())
}

/// Open a chosen URI via the `open` command.
pub async fn open_uri(
    backends: &mut mz::Backends,
    archive: &d2mz_archive::Archive,
    uri: &str,
) -> Result<()> {
    let args = crate::cli::OpenArgs {
        uri: uri.to_string(),
        with: None,
        as_kind: None,
        print: false,
    };
    crate::commands::open::run(backends, archive, args).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_keeps_uri_and_display() {
        let candidate = Candidate::new("mz://local/a.txt", "a.txt");
        assert_eq!(candidate.uri, "mz://local/a.txt");
        assert_eq!(candidate.display, "a.txt");
    }

    #[test]
    fn tty_is_reportable() {
        // Just ensure the call is safe in a test process.
        let _ = is_tty();
        let _ = available();
    }

    #[test]
    fn parses_enter_as_open_by_default() {
        // fzf writes an empty key line for Enter.
        let out = "\nmz://local/a.txt\talpha.txt\n";
        assert_eq!(
            parse_choice(out, DefaultAction::Open),
            Some(("mz://local/a.txt".to_string(), Action::Open))
        );
    }

    #[test]
    fn parses_print_key() {
        let out = "ctrl-p\nmz://local/a.txt\talpha.txt\n";
        assert_eq!(
            parse_choice(out, DefaultAction::Open),
            Some(("mz://local/a.txt".to_string(), Action::Print))
        );
        // With print as the default, the key flips to open.
        assert_eq!(
            parse_choice(out, DefaultAction::Print),
            Some(("mz://local/a.txt".to_string(), Action::Open))
        );
    }

    #[test]
    fn parses_alt_enter_as_print_key() {
        let out = "alt-enter\nmz://local/a.txt\talpha.txt\n";
        assert_eq!(
            parse_choice(out, DefaultAction::Open),
            Some(("mz://local/a.txt".to_string(), Action::Print))
        );
    }

    #[test]
    fn aborts_on_empty_output() {
        assert_eq!(parse_choice("", DefaultAction::Open), None);
        // A key line with no chosen row is also an abort.
        assert_eq!(parse_choice("ctrl-p\n", DefaultAction::Open), None);
    }
}
