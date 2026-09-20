//! Tests for interactive selection behaviour in non-terminal contexts.
//!
//! Running the real fzf needs a terminal, so these tests cover the fallback
//! paths: piping, `--json` and `--no-interactive` must all print the plain
//! listing, never start a picker.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn d2mz(archive: &Path) -> Command {
    let config = archive.join("config.toml");
    fs::write(
        &config,
        format!(
            "archive_dir = {:?}\n\n[[backend]]\nname = \"local\"\nscheme = \"fs\"\nroot = \"/\"\n",
            archive.join("data").to_string_lossy()
        ),
    )
    .expect("write config");
    let mut command = Command::new(env!("CARGO_BIN_EXE_d2mz"));
    command.arg("--config").arg(&config);
    command
}

fn run(command: &mut Command) -> Output {
    let output = command.output().expect("spawn d2mz");
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf-8 stdout")
}

/// An archive with two ingested files.
fn fixture() -> (tempfile::TempDir, tempfile::TempDir) {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    fs::write(src.path().join("alpha.txt"), "a\n").unwrap();
    fs::write(src.path().join("beta.txt"), "b\n").unwrap();
    run(d2mz(archive.path()).args(["ingest", "-R"]).arg(src.path()));
    (archive, src)
}

#[test]
fn search_falls_back_to_plain_output_when_piped() {
    let (archive, _src) = fixture();
    // No TTY: the picker is skipped and the listing is printed.
    let listing = stdout(&run(d2mz(archive.path()).args(["search", "txt"])));
    assert!(listing.contains("alpha.txt"), "{listing}");
    assert!(listing.contains("beta.txt"), "{listing}");
}

#[test]
fn search_no_interactive_prints_plain() {
    let (archive, _src) = fixture();
    let listing = stdout(&run(d2mz(archive.path()).args([
        "search",
        "txt",
        "--no-interactive",
    ])));
    assert!(listing.contains("alpha.txt"), "{listing}");
}

#[test]
fn search_json_never_starts_a_picker() {
    let (archive, _src) = fixture();
    let listing = stdout(&run(d2mz(archive.path()).args(["search", "txt", "--json"])));
    let records: Vec<serde_json::Value> = serde_json::from_str(&listing).unwrap();
    assert_eq!(records.len(), 2);
}

#[test]
fn archive_and_find_also_fall_back() {
    let (archive, src) = fixture();

    let archived = stdout(&run(d2mz(archive.path()).args(["archive"])));
    assert!(archived.contains("alpha.txt"), "{archived}");

    let found = stdout(&run(d2mz(archive.path()).args(["find"]).arg(src.path())));
    assert!(found.contains("alpha.txt"), "{found}");
    assert!(found.contains("beta.txt"), "{found}");
}
