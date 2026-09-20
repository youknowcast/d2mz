//! End-to-end tests for syncing against a remote main database.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// A d2mz command bound to its own archive directory.
fn d2mz(home: &Path, archive: &str) -> Command {
    let archive_dir = home.join(archive);
    fs::create_dir_all(&archive_dir).unwrap();
    let config = home.join(format!("{archive}.toml"));
    fs::write(
        &config,
        format!(
            "archive_dir = {:?}\n\n[[backend]]\nname = \"local\"\nscheme = \"fs\"\nroot = \"/\"\n",
            archive_dir.to_string_lossy()
        ),
    )
    .unwrap();
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

/// Remote snapshot path inside the shared home directory.
fn remote(home: &Path) -> String {
    format!("mz://local{}/main.db", home.display())
}

#[test]
fn two_nodes_converge_through_the_remote() {
    let home = tempfile::tempdir().unwrap();

    // Node 1 registers a file and seeds the remote.
    let src1 = home.path().join("s1");
    fs::create_dir_all(&src1).unwrap();
    fs::write(src1.join("a.txt"), "from node one\n").unwrap();
    run(d2mz(home.path(), "a1")
        .args(["ingest"])
        .arg(src1.join("a.txt")));
    run(d2mz(home.path(), "a1").args(["sync", "--remote", &remote(home.path()), "--init"]));

    // Node 2 joins and pulls node 1's entry.
    run(d2mz(home.path(), "a2").args(["sync", "--remote", &remote(home.path())]));
    let listed = stdout(&run(d2mz(home.path(), "a2").args(["search", "txt"])));
    assert!(listed.contains("a.txt"), "{listed}");

    // Node 2 registers its own file and pushes it.
    let src2 = home.path().join("s2");
    fs::create_dir_all(&src2).unwrap();
    fs::write(src2.join("b.txt"), "from node two\n").unwrap();
    run(d2mz(home.path(), "a2")
        .args(["ingest"])
        .arg(src2.join("b.txt")));
    run(d2mz(home.path(), "a2").args(["sync", "--remote", &remote(home.path())]));

    // Node 1 sees both.
    run(d2mz(home.path(), "a1").args(["sync", "--remote", &remote(home.path())]));
    let all = stdout(&run(
        d2mz(home.path(), "a1").args(["search", "txt", "--json"])
    ));
    let records: Vec<serde_json::Value> = serde_json::from_str(&all).unwrap();
    let names: Vec<&str> = records
        .iter()
        .map(|record| record["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"a.txt"), "{names:?}");
    assert!(names.contains(&"b.txt"), "{names:?}");
}

#[test]
fn sync_is_a_no_op_when_nothing_changed() {
    let home = tempfile::tempdir().unwrap();
    let src = home.path().join("s");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), "hello\n").unwrap();

    run(d2mz(home.path(), "a1")
        .args(["ingest"])
        .arg(src.join("a.txt")));
    run(d2mz(home.path(), "a1").args(["sync", "--remote", &remote(home.path()), "--init"]));

    let second = stdout(&run(d2mz(home.path(), "a1").args([
        "sync",
        "--remote",
        &remote(home.path()),
        "--json",
    ])));
    let report: serde_json::Value = serde_json::from_str(&second).unwrap();
    assert_eq!(report["pulled_entries"], 0);
    assert_eq!(report["pushed_entries"], 0);
}

#[test]
fn init_refuses_to_overwrite_an_existing_main() {
    let home = tempfile::tempdir().unwrap();
    let src = home.path().join("s");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), "hello\n").unwrap();

    run(d2mz(home.path(), "a1")
        .args(["ingest"])
        .arg(src.join("a.txt")));
    run(d2mz(home.path(), "a1").args(["sync", "--remote", &remote(home.path()), "--init"]));

    // A second --init must not silently wipe the catalogue.
    let blocked = d2mz(home.path(), "a1")
        .args(["sync", "--remote", &remote(home.path()), "--init"])
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(stderr.contains("already exists"), "{stderr}");

    // --force makes the intent explicit.
    run(d2mz(home.path(), "a1").args([
        "sync",
        "--remote",
        &remote(home.path()),
        "--init",
        "--force",
    ]));
}

#[test]
fn a_different_main_database_is_rejected() {
    let home = tempfile::tempdir().unwrap();
    let src = home.path().join("s");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), "one\n").unwrap();

    // Node 1 binds to main A.
    run(d2mz(home.path(), "a1")
        .args(["ingest"])
        .arg(src.join("a.txt")));
    let main_a = format!("mz://local{}/mainA.db", home.path().display());
    run(d2mz(home.path(), "a1").args(["sync", "--remote", &main_a, "--init"]));
    run(d2mz(home.path(), "a1").args(["sync", "--remote", &main_a]));

    // A second node creates a different main B.
    let main_b = format!("mz://local{}/mainB.db", home.path().display());
    fs::write(src.join("b.txt"), "two\n").unwrap();
    run(d2mz(home.path(), "a2")
        .args(["ingest"])
        .arg(src.join("b.txt")));
    run(d2mz(home.path(), "a2").args(["sync", "--remote", &main_b, "--init"]));

    // Node 1 must not merge against B.
    let blocked = d2mz(home.path(), "a1")
        .args(["sync", "--remote", &main_b])
        .output()
        .unwrap();
    assert!(!blocked.status.success());
    let stderr = String::from_utf8_lossy(&blocked.stderr);
    assert!(stderr.contains("different main database"), "{stderr}");
}

#[test]
fn sync_uses_the_configured_main() {
    let home = tempfile::tempdir().unwrap();
    let src = home.path().join("s");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), "hello\n").unwrap();

    // Write a config with `main` so --remote is not needed.
    let archive_dir = home.path().join("cfg");
    fs::create_dir_all(&archive_dir).unwrap();
    let config = home.path().join("cfg.toml");
    fs::write(
        &config,
        format!(
            "archive_dir = {:?}\nmain = {:?}\n\n[[backend]]\nname = \"local\"\nscheme = \"fs\"\nroot = \"/\"\n",
            archive_dir.to_string_lossy(),
            remote(home.path()),
        ),
    )
    .unwrap();

    let d2mz_cfg = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_d2mz"));
        command.arg("--config").arg(&config);
        command
    };

    run(d2mz_cfg().args(["ingest"]).arg(src.join("a.txt")));
    run(d2mz_cfg().args(["sync", "--init"]));

    let report = stdout(&run(d2mz_cfg().args(["sync", "--json"])));
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(parsed["pulled_entries"], 0);
}

#[test]
fn tags_and_meta_propagate() {
    let home = tempfile::tempdir().unwrap();
    let src = home.path().join("s");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), "hello\n").unwrap();

    run(d2mz(home.path(), "a1")
        .args(["ingest"])
        .arg(src.join("a.txt")));
    run(d2mz(home.path(), "a1").args(["tag", "add", "work", "--id", "a.txt"]));
    run(d2mz(home.path(), "a1").args(["sync", "--remote", &remote(home.path()), "--init"]));

    run(d2mz(home.path(), "a2").args(["sync", "--remote", &remote(home.path())]));
    let tagged = stdout(&run(d2mz(home.path(), "a2").args(["tag", "list", "work"])));
    assert!(tagged.contains("a.txt"), "{tagged}");
}

#[test]
fn auto_sync_pushes_after_an_index_change() {
    let home = tempfile::tempdir().unwrap();
    let archive_dir = home.path().join("auto");
    fs::create_dir_all(&archive_dir).unwrap();
    let config = home.path().join("auto.toml");
    fs::write(
        &config,
        format!(
            "archive_dir = {:?}\nmain = {:?}\nauto_sync = true\n\n[[backend]]\nname = \"local\"\nscheme = \"fs\"\nroot = \"/\"\n",
            archive_dir.to_string_lossy(),
            remote(home.path()),
        ),
    )
    .unwrap();

    let d2mz_cfg = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_d2mz"));
        command.arg("--config").arg(&config);
        command
    };

    // Seed the main database, then ingest: the push should be automatic.
    run(d2mz_cfg().args(["sync", "--init"]));
    let src = home.path().join("s");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), "hello\n").unwrap();
    run(d2mz_cfg().args(["ingest"]).arg(src.join("a.txt")));

    // A fresh node that only syncs must see the entry.
    let other = tempfile::tempdir().unwrap();
    let other_config = other.path().join("other.toml");
    fs::write(
        &other_config,
        format!(
            "archive_dir = {:?}\nmain = {:?}\n\n[[backend]]\nname = \"local\"\nscheme = \"fs\"\nroot = \"/\"\n",
            other.path().join("data").to_string_lossy(),
            remote(home.path()),
        ),
    )
    .unwrap();
    let mut sync_other = Command::new(env!("CARGO_BIN_EXE_d2mz"));
    sync_other.arg("--config").arg(&other_config).arg("sync");
    run(&mut sync_other);

    let mut search = Command::new(env!("CARGO_BIN_EXE_d2mz"));
    search
        .arg("--config")
        .arg(&other_config)
        .args(["search", "a"]);
    let found = stdout(&run(&mut search));
    assert!(found.contains("a.txt"), "{found}");
}
