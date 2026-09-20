//! End-to-end tests exercising the CLI binary against a local backend.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

/// A command pointing at a non-existent config so tests are hermetic.
fn d2mz() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_d2mz"));
    command
        .arg("--config")
        .arg("/nonexistent/d2mz-test/config.toml");
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

fn fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), "hello world").unwrap();
    fs::write(dir.path().join("skip.md"), "# title").unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("nested").join("deep.log"), "log").unwrap();
    dir
}

#[test]
fn ls_lists_immediate_children() {
    let dir = fixture();
    let output = run(d2mz().arg("ls").arg(dir.path()));
    let listing = stdout(&output);

    assert!(listing.contains("notes.txt"), "{listing}");
    assert!(listing.contains("skip.md"), "{listing}");
    assert!(listing.contains("nested/"), "{listing}");
    // Only immediate children, not the directory itself.
    assert!(!listing.contains("deep.log"), "{listing}");
    assert!(!listing.contains("nested\n"), "{listing}");
}

#[test]
fn ls_json_reports_paths() {
    let dir = fixture();
    let output = run(d2mz().args(["ls", "--json"]).arg(dir.path()));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");

    let names: Vec<&str> = parsed
        .as_array()
        .expect("array")
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"notes.txt"), "{names:?}");
    assert!(names.contains(&"nested"), "{names:?}");
}

#[test]
fn ls_json_is_round_trippable() {
    let dir = fixture();
    let output = run(d2mz().args(["ls", "--json"]).arg(dir.path()));
    let views: Vec<d2mz::output::EntryView> =
        serde_json::from_str(&stdout(&output)).expect("deserializes into entry views");
    assert!(!views.is_empty());
    assert!(
        views
            .iter()
            .all(|view| view.kind == "file" || view.kind == "dir")
    );
}

#[test]
fn cat_streams_file_contents() {
    let dir = fixture();
    let file = dir.path().join("notes.txt");
    let output = run(d2mz().arg("cat").arg(&file));
    assert_eq!(stdout(&output), "hello world");
}

#[test]
fn stat_json_has_size() {
    let dir = fixture();
    let file = dir.path().join("notes.txt");
    let output = run(d2mz().args(["stat", "--json"]).arg(&file));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("valid JSON");

    assert_eq!(parsed["kind"], "file");
    assert_eq!(parsed["size"], 11);
}

#[test]
fn unknown_backend_fails() {
    let output = d2mz()
        .args(["ls", "mz://does-not-exist/x"])
        .output()
        .expect("spawn d2mz");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown backend"), "{stderr}");
}

/// Guard against accidental path handling regressions.
#[test]
fn ls_of_file_shows_itself() {
    let dir = fixture();
    let file: &Path = &dir.path().join("notes.txt");
    let listing = stdout(&run(d2mz().arg("ls").arg(file)));
    assert_eq!(listing.trim(), "notes.txt");
}

#[test]
fn ls_recursive_descends_into_subdirectories() {
    let dir = fixture();
    let listing = stdout(&run(d2mz().args(["ls", "-R"]).arg(dir.path())));

    assert!(listing.contains("notes.txt"), "{listing}");
    assert!(listing.contains("deep.log"), "{listing}");
    assert!(listing.contains("nested/"), "{listing}");
}

#[test]
fn find_filters_by_name_glob() {
    let dir = fixture();
    let listing = stdout(&run(d2mz()
        .args(["find", "--name", "*.log"])
        .arg(dir.path())));

    assert!(listing.contains("deep.log"), "{listing}");
    assert!(!listing.contains("notes.txt"), "{listing}");
    assert!(!listing.contains("skip.md"), "{listing}");
}

#[test]
fn find_accepts_comma_separated_globs() {
    let dir = fixture();
    let listing = stdout(&run(d2mz()
        .args(["find", "--name", "*.log, *.md"])
        .arg(dir.path())));

    assert!(listing.contains("deep.log"), "{listing}");
    assert!(listing.contains("skip.md"), "{listing}");
    assert!(!listing.contains("notes.txt"), "{listing}");
}

#[test]
fn find_filters_by_size() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("small.txt"), "x").unwrap();
    fs::write(dir.path().join("large.txt"), "x".repeat(4096)).unwrap();

    let big = stdout(&run(d2mz()
        .args(["find", "--min-size", "1K"])
        .arg(dir.path())));
    assert!(big.contains("large.txt"), "{big}");
    assert!(!big.contains("small.txt"), "{big}");

    let small = stdout(&run(d2mz()
        .args(["find", "--max-size", "1K"])
        .arg(dir.path())));
    assert!(small.contains("small.txt"), "{small}");
    assert!(!small.contains("large.txt"), "{small}");
}

#[test]
fn find_json_reports_paths() {
    let dir = fixture();
    let listing = stdout(&run(d2mz()
        .args(["find", "--name", "*.log", "--json"])
        .arg(dir.path())));
    let parsed: serde_json::Value = serde_json::from_str(&listing).expect("valid JSON");
    let names: Vec<&str> = parsed
        .as_array()
        .expect("array")
        .iter()
        .map(|entry| entry["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"deep.log"), "{names:?}");
}
