//! End-to-end tests for stat enrichment, ingest summaries and mtime restore.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

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
fn stat_reports_archived_hash_state_and_tags() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("note.txt");
    fs::write(&file, "hello\n").unwrap();

    // Before ingest, stat says so.
    let before = stdout(&run(d2mz(archive.path()).args(["stat"]).arg(&file)));
    assert!(before.contains("archived:     no"), "{before}");

    run(d2mz(archive.path()).args(["ingest"]).arg(&file));
    run(d2mz(archive.path()).args(["tag", "add", "work", "--id", "note.txt"]));

    let after = stdout(&run(d2mz(archive.path()).args(["stat"]).arg(&file)));
    assert!(after.contains("hash:"), "{after}");
    assert!(after.contains("state:        present"), "{after}");
    assert!(after.contains("tags:         work"), "{after}");
}

#[test]
fn ingest_prints_a_summary_line() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    fs::write(src.path().join("a.txt"), "one\n").unwrap();
    fs::write(src.path().join("b.txt"), "two\n").unwrap();

    let output = run(d2mz(archive.path()).args(["ingest", "-R"]).arg(src.path()));
    let err = stderr(&output);
    assert!(err.contains("2 ingested"), "{err}");

    // A second pass deduplicates both.
    let again = run(d2mz(archive.path()).args(["ingest", "-R"]).arg(src.path()));
    assert!(
        stderr(&again).contains("2 deduplicated"),
        "{}",
        stderr(&again)
    );
}

#[test]
fn export_restores_the_recorded_mtime() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("note.txt");
    fs::write(&file, "hello\n").unwrap();

    // Give the source an unmistakable mtime in the past.
    let past = SystemTime::now() - Duration::from_secs(86_400 * 30);
    fs::File::open(&file).unwrap().set_modified(past).unwrap();

    run(d2mz(archive.path()).args(["ingest"]).arg(&file));

    let listed = stdout(&run(d2mz(archive.path()).args(["archive", "--json"])));
    let records: Vec<serde_json::Value> = serde_json::from_str(&listed).unwrap();
    let hash = records[0]["hash"].as_str().unwrap().to_string();

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("restored.txt");
    run(d2mz(archive.path()).args(["export", &hash]).arg(&dest));

    let restored = fs::metadata(&dest).unwrap().modified().unwrap();
    let age = SystemTime::now()
        .duration_since(restored)
        .unwrap_or_default();
    assert!(
        age > Duration::from_secs(86_400 * 29),
        "mtime was not restored (age {age:?})"
    );
}
