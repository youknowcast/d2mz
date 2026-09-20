//! End-to-end tests for feedback and exit codes.

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

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf-8 stderr")
}

#[test]
fn cat_refuses_binary_and_force_overrides() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("blob.bin");
    fs::write(&file, [0u8, 1, 2, 3, 0, 4]).unwrap();

    let blocked = d2mz(archive.path())
        .args(["cat"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    assert!(
        stderr(&blocked).contains("looks binary"),
        "{}",
        stderr(&blocked)
    );

    // --force still prints it.
    let forced = run(d2mz(archive.path()).args(["cat", "--force"]).arg(&file));
    assert_eq!(forced.stdout, vec![0u8, 1, 2, 3, 0, 4]);
}

#[test]
fn cat_prints_text_without_a_warning() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("note.txt");
    fs::write(&file, "plain text\n").unwrap();

    let output = run(d2mz(archive.path()).args(["cat"]).arg(&file));
    assert_eq!(stdout(&output), "plain text\n");
}

#[test]
fn search_exits_nonzero_when_nothing_matches() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("a.txt");
    fs::write(&file, "hello\n").unwrap();
    run(d2mz(archive.path()).args(["ingest"]).arg(&file));

    // A hit is success (FTS indexes names, paths, sources and tags).
    let hit = d2mz(archive.path()).args(["search", "a"]).output().unwrap();
    assert!(hit.status.success(), "{}", stderr(&hit));

    // A miss is exit 1, like grep.
    let miss = d2mz(archive.path())
        .args(["search", "nomatchhere"])
        .output()
        .unwrap();
    assert_eq!(miss.status.code(), Some(1), "{}", stderr(&miss));
}

#[test]
fn find_exits_nonzero_when_nothing_matches() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    fs::write(src.path().join("a.txt"), "hello\n").unwrap();

    let miss = d2mz(archive.path())
        .args(["find"])
        .arg(src.path())
        .args(["--name", "*.none"])
        .output()
        .unwrap();
    assert_eq!(miss.status.code(), Some(1));
}

#[test]
fn open_reports_the_command_and_missing_opener() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("photo.png");
    fs::write(&file, b"\x89PNG\r\n\x1a\n").unwrap();

    // Point at a handler that does not exist; d2mz must name the program and
    // point at `handler set` rather than failing obscurely.
    run(d2mz(archive.path()).args([
        "handler",
        "set",
        "image",
        "--app",
        "definitely-not-a-real-opener {}",
    ]));
    let output = d2mz(archive.path())
        .args(["open"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("not on PATH"), "{err}");
    assert!(err.contains("handler set"), "{err}");
}
