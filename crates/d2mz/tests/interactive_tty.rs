//! Interactive picker tests driven through a pseudo-terminal.
//!
//! Gated behind `test-support` and skipped unless both `fzf` and `script`
//! are available, since a real terminal is required.

#![cfg(feature = "test-support")]

use std::fs;
use std::path::Path;
use std::process::Command;

fn have(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
}

fn d2mz(archive: &Path) -> Command {
    let config = archive.join("config.toml");
    fs::write(
        &config,
        format!(
            "archive_dir = {:?}\n\n[[backend]]\nname = \"local\"\nscheme = \"fs\"\nroot = \"/\"\n",
            archive.join("data").to_string_lossy()
        ),
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_d2mz"));
    command.arg("--config").arg(&config);
    command
}

/// Run a command under `script` with delayed keystrokes, returning printable
/// output (ANSI stripped).
///
/// The keystrokes are piped into `script`'s standard input, which is the
/// pseudo-terminal fzf reads from.
fn run_tty(command: &mut Command, keys: &[(u64, &str)]) -> String {
    let mut feed = String::new();
    for (delay, key) in keys {
        let escaped = match *key {
            "\r" => "\\r".to_string(),
            "\u{10}" => "\\020".to_string(),
            other => other.to_string(),
        };
        feed.push_str(&format!("sleep {delay}; printf '{escaped}'; "));
    }

    let mut script = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "( {feed} ) | script -qec {cmd} /dev/null",
            cmd = shell_quote(&shell_join(command))
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn script");
    drop(script.stdin.take());
    let output = script.wait_with_output().unwrap();

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    strip_ansi(&raw)
}

/// Single-quote a string for the shell.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Join a Command into a shell string for `script -c`.
fn shell_join(command: &Command) -> String {
    let mut parts = vec![command.get_program().to_string_lossy().to_string()];
    for arg in command.get_args() {
        let arg = arg.to_string_lossy().to_string();
        if arg.contains(' ') {
            parts.push(format!("'{arg}'"));
        } else {
            parts.push(arg);
        }
    }
    parts.join(" ")
}

/// Remove ANSI escape sequences so assertions see plain text.
fn strip_ansi(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn fixture() -> (tempfile::TempDir, tempfile::TempDir) {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    fs::write(src.path().join("alpha.txt"), "a\n").unwrap();
    fs::write(src.path().join("beta.txt"), "b\n").unwrap();
    let out = d2mz(archive.path())
        .args(["ingest", "-R"])
        .arg(src.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    (archive, src)
}

#[test]
fn enter_prints_the_uri_with_print_default() {
    if !have("fzf") || !have("script") {
        eprintln!("fzf or script missing; skipping");
        return;
    }
    let (archive, _src) = fixture();
    let mut cmd = d2mz(archive.path());
    cmd.args(["search", "txt", "--print"]);

    let output = run_tty(&mut cmd, &[(1, "alpha"), (1, "\r")]);
    assert!(
        output.contains("mz://") && output.contains("alpha.txt"),
        "{output}"
    );
}

#[test]
fn enter_opens_with_the_default_action() {
    if !have("fzf") || !have("script") {
        eprintln!("fzf or script missing; skipping");
        return;
    }
    let (archive, _src) = fixture();
    let mut handler = d2mz(archive.path());
    handler.args(["handler", "set", "text", "--app", "echo OPENED {}"]);
    assert!(handler.output().unwrap().status.success());

    let mut cmd = d2mz(archive.path());
    cmd.args(["search", "txt"]);
    let output = run_tty(&mut cmd, &[(1, "beta"), (1, "\r")]);
    assert!(output.contains("OPENED"), "{output}");
    assert!(output.contains("beta.txt"), "{output}");
}

#[test]
fn ctrl_p_prints_instead_of_opening() {
    if !have("fzf") || !have("script") {
        eprintln!("fzf or script missing; skipping");
        return;
    }
    let (archive, _src) = fixture();
    let mut handler = d2mz(archive.path());
    handler.args(["handler", "set", "text", "--app", "echo OPENED {}"]);
    assert!(handler.output().unwrap().status.success());

    let mut cmd = d2mz(archive.path());
    cmd.args(["search", "txt"]);
    // Ctrl-P is 0x10.
    let output = run_tty(&mut cmd, &[(1, "beta"), (1, "\u{10}")]);
    assert!(output.contains("mz://"), "{output}");
    assert!(!output.contains("OPENED"), "{output}");
}
