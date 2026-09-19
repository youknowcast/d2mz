//! End-to-end tests for the content-addressed archive.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

/// A d2mz command with a temp archive directory.
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

/// Source tree with a duplicate so dedup can be observed.
fn source() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    fs::write(dir.path().join("b.txt"), "beta\n").unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("nested").join("dup.txt"), "alpha\n").unwrap();
    dir
}

#[test]
fn ingest_deduplicates_identical_contents() {
    let archive = tempfile::tempdir().unwrap();
    let src = source();

    run(d2mz(archive.path()).args(["ingest", "-R"]).arg(src.path()));

    let listing = stdout(&run(d2mz(archive.path()).args(["archive", "--json"])));
    let records: Vec<serde_json::Value> = serde_json::from_str(&listing).expect("valid JSON");
    assert_eq!(records.len(), 3, "three entries recorded");

    // Two entries share the hash of "alpha\n", the third does not.
    let mut hashes: Vec<&str> = records
        .iter()
        .map(|record| record["hash"].as_str().unwrap())
        .collect();
    hashes.sort();
    assert_eq!(hashes[1], hashes[2], "duplicate contents share a blob");
    assert_ne!(hashes[0], hashes[1]);

    // Only two blobs were written.
    let store = archive.path().join("data").join("store");
    let blobs = count_files(&store);
    assert_eq!(blobs, 2, "duplicate blob stored once");
}

#[test]
fn ingest_respects_name_filter() {
    let archive = tempfile::tempdir().unwrap();
    let src = source();

    run(d2mz(archive.path())
        .args(["ingest", "-R", "--name", "*.txt"])
        .arg(src.path()));

    let records: Vec<serde_json::Value> = serde_json::from_str(&stdout(&run(
        d2mz(archive.path()).args(["archive", "--json"])
    )))
    .unwrap();
    assert_eq!(records.len(), 3);
}

#[test]
fn archive_lists_under_prefix() {
    let archive = tempfile::tempdir().unwrap();
    let src = source();
    run(d2mz(archive.path()).args(["ingest", "-R"]).arg(src.path()));

    let nested = format!("{}", src.path().join("nested").display());
    let listing = stdout(&run(d2mz(archive.path())
        .args(["archive", "--prefix"])
        .arg(prefix_of(&nested))));
    assert_eq!(listing.lines().count(), 1, "{listing}");
    assert!(listing.contains("dup.txt"), "{listing}");
}

#[test]
fn export_round_trips_a_blob() {
    let archive = tempfile::tempdir().unwrap();
    let src = source();
    run(d2mz(archive.path()).args(["ingest", "-R"]).arg(src.path()));

    let records: Vec<serde_json::Value> = serde_json::from_str(&stdout(&run(
        d2mz(archive.path()).args(["archive", "--json"])
    )))
    .unwrap();
    let hash = records[0]["hash"].as_str().unwrap();

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("restored.txt");
    run(d2mz(archive.path()).args(["export", hash]).arg(&dest));

    let restored = fs::read_to_string(&dest).unwrap();
    assert!(
        restored == "alpha\n" || restored == "beta\n",
        "{restored:?}"
    );
}

#[test]
fn tags_can_be_added_listed_and_removed() {
    let archive = tempfile::tempdir().unwrap();
    let src = source();
    run(d2mz(archive.path()).args(["ingest", "-R"]).arg(src.path()));

    run(d2mz(archive.path()).args(["tag", "add", "work", "--id", "a.txt"]));

    let tagged = stdout(&run(d2mz(archive.path()).args(["tag", "list", "work"])));
    assert!(tagged.contains("a.txt"), "{tagged}");

    let all = stdout(&run(d2mz(archive.path()).args(["tag", "list"])));
    assert!(all.contains("work"), "{all}");

    run(d2mz(archive.path()).args(["tag", "rm", "work", "--id", "a.txt"]));
    let after = stdout(&run(d2mz(archive.path()).args(["tag", "list"])));
    assert!(!after.contains("work"), "{after}");
}

#[test]
fn search_finds_by_tag_and_name() {
    let archive = tempfile::tempdir().unwrap();
    let src = source();
    run(d2mz(archive.path()).args(["ingest", "-R"]).arg(src.path()));
    run(d2mz(archive.path()).args(["tag", "add", "holiday", "--id", "b.txt"]));

    let by_tag = stdout(&run(d2mz(archive.path()).args(["search", "holiday"])));
    assert!(by_tag.contains("b.txt"), "{by_tag}");

    let by_name = stdout(&run(d2mz(archive.path()).args(["search", "dup"])));
    assert!(by_name.contains("dup.txt"), "{by_name}");

    let miss = stdout(&run(
        d2mz(archive.path()).args(["search", "holiday", "--json"])
    ));
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&miss).unwrap();
    assert_eq!(parsed.len(), 1);
}

#[test]
fn search_is_cross_backend_and_needs_no_uri() {
    // Two separate source trees stand in for two backends. Search never
    // touches them again; it only reads the local index.
    let archive = tempfile::tempdir().unwrap();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    fs::write(first.path().join("alpha-report.txt"), "a\n").unwrap();
    fs::write(second.path().join("beta-report.txt"), "b\n").unwrap();

    run(d2mz(archive.path())
        .args(["ingest"])
        .arg(first.path().join("alpha-report.txt")));
    run(d2mz(archive.path())
        .args(["ingest"])
        .arg(second.path().join("beta-report.txt")));

    let listing = stdout(&run(d2mz(archive.path()).args(["search", "report"])));
    assert!(listing.contains("alpha-report.txt"), "{listing}");
    assert!(listing.contains("beta-report.txt"), "{listing}");

    // The default output names the backend for each hit; a leading space is
    // the "present" state marker.
    let line = listing.lines().next().unwrap();
    assert!(line.trim_start().starts_with("local"), "{line}");

    // JSON exposes the backend too.
    let json = stdout(&run(
        d2mz(archive.path()).args(["search", "report", "--json"])
    ));
    let records: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
    assert!(records.iter().all(|record| record["backend"] == "local"));
}

#[test]
fn scan_tracks_missing_and_reappearing_sources() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("note.txt");
    fs::write(&file, "hello\n").unwrap();

    run(d2mz(archive.path()).args(["ingest"]).arg(&file));
    run(d2mz(archive.path()).args(["scan"]));

    let present = entries(&archive);
    assert_eq!(present[0]["state"], "present");

    fs::remove_file(&file).unwrap();
    run(d2mz(archive.path()).args(["scan"]));
    let missing = entries(&archive);
    assert_eq!(missing[0]["state"], "missing");

    // The blob survives a disappearance, so a reappearing file is cheap.
    fs::write(&file, "hello\n").unwrap();
    run(d2mz(archive.path()).args(["scan"]));
    let back = entries(&archive);
    assert_eq!(back[0]["state"], "present");
    assert_eq!(missing[0]["hash"], back[0]["hash"]);
}

#[test]
fn scan_detects_changed_contents() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("note.txt");
    fs::write(&file, "v1\n").unwrap();
    run(d2mz(archive.path()).args(["ingest"]).arg(&file));
    let before = entries(&archive)[0]["hash"].as_str().unwrap().to_string();

    fs::write(&file, "v2 changed\n").unwrap();
    run(d2mz(archive.path()).args(["scan"]));
    let after = entries(&archive)[0]["hash"].as_str().unwrap().to_string();

    assert_ne!(before, after);
    assert_eq!(count_files(&archive.path().join("data").join("store")), 2);
}

#[test]
fn forget_retires_an_entry() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("note.txt");
    fs::write(&file, "hello\n").unwrap();
    run(d2mz(archive.path()).args(["ingest"]).arg(&file));

    run(d2mz(archive.path()).args(["forget", "--id", "note.txt"]));
    let retired = stdout(&run(
        d2mz(archive.path()).args(["archive", "--state", "deleted", "--json"])
    ));
    let records: Vec<serde_json::Value> = serde_json::from_str(&retired).unwrap();
    assert_eq!(records.len(), 1);

    // A later scan must not resurrect a retired entry.
    run(d2mz(archive.path()).args(["scan"]));
    let after = entries(&archive);
    assert_eq!(after[0]["state"], "deleted");
}

#[test]
fn reingesting_same_source_does_not_duplicate() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("note.txt");
    fs::write(&file, "hello\n").unwrap();

    run(d2mz(archive.path()).args(["ingest"]).arg(&file));
    run(d2mz(archive.path()).args(["ingest"]).arg(&file));

    assert_eq!(entries(&archive).len(), 1);
}

/// All entries as JSON values.
fn entries(archive: &TempDir) -> Vec<serde_json::Value> {
    let json = stdout(&run(d2mz(archive.path()).args(["archive", "--json"])));
    serde_json::from_str(&json).unwrap()
}

fn count_files(dir: &Path) -> usize {
    let mut count = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                count += 1;
            }
        }
    }
    count
}

/// Backend paths have no leading slash; mirror that for the prefix filter.
fn prefix_of(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}
