//! End-to-end tests for reading and restoring vanished sources.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

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

fn fixture() -> (TempDir, TempDir, std::path::PathBuf) {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("note.txt");
    fs::write(&file, "important contents\n").unwrap();
    run(d2mz(archive.path()).args(["ingest"]).arg(&file));
    (archive, src, file)
}

/// Ingest and then remove the source, so the index records it as missing.
fn vanished() -> (TempDir, TempDir, std::path::PathBuf) {
    let (archive, src, file) = fixture();
    fs::remove_file(&file).unwrap();
    run(d2mz(archive.path()).args(["scan"]));
    (archive, src, file)
}

#[test]
fn cat_reads_the_archive_when_the_source_is_gone() {
    let (archive, _src, file) = vanished();

    let output = run(d2mz(archive.path()).args(["cat"]).arg(&file));
    assert_eq!(stdout(&output), "important contents\n");
}

#[test]
fn export_restores_a_missing_file_in_place() {
    let (archive, _src, file) = vanished();

    // Find the hash, then export with no destination: the missing source is
    // the target, so the file returns to where it came from.
    let listed = stdout(&run(d2mz(archive.path()).args(["archive", "--json"])));
    let records: Vec<serde_json::Value> = serde_json::from_str(&listed).unwrap();
    let hash = records[0]["hash"].as_str().unwrap().to_string();
    run(d2mz(archive.path()).args(["export", &hash]));

    assert!(file.exists(), "file was not restored to {}", file.display());
    assert_eq!(fs::read_to_string(&file).unwrap(), "important contents\n");

    // Restoring the same contents again is a no-op.
    let output = run(d2mz(archive.path()).args(["export", &hash]));
    assert!(
        stdout(&output).contains("nothing to do"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn open_print_falls_back_to_the_archived_blob() {
    let (archive, _src, file) = vanished();

    let printed = stdout(&run(d2mz(archive.path())
        .args(["open", "--print"])
        .arg(&file)));
    assert!(Path::new(printed.trim()).exists(), "{printed}");
    assert!(printed.contains("store"), "{printed}");
}
