//! End-to-end tests for thumbnail generation and caching.

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

/// Write a small valid PNG without pulling in an image library.
fn write_png(path: &Path, width: u32, height: u32) {
    use std::io::Write;

    fn chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&(crc32(&body)).to_be_bytes());
        out
    }

    // 8-bit RGB, no interlace.
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);

    // One filter byte plus RGB triples per row.
    let mut raw = Vec::new();
    for y in 0..height {
        raw.push(0);
        for x in 0..width {
            raw.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 128]);
        }
    }

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&chunk(b"IDAT", &zlib_store(&raw)));
    png.extend_from_slice(&chunk(b"IEND", &[]));

    let mut file = fs::File::create(path).unwrap();
    file.write_all(&png).unwrap();
}

/// A minimal zlib stream using stored (uncompressed) deflate blocks.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    for block in data.chunks(65535) {
        let last = (block.as_ptr() as usize + block.len()) == (data.as_ptr() as usize + data.len());
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(block.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

#[test]
fn ingest_generates_thumbnails_by_default() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let image = src.path().join("photo.png");
    let text = src.path().join("notes.txt");
    write_png(&image, 320, 240);
    fs::write(&text, "hello\n").unwrap();

    run(d2mz(archive.path()).args(["ingest"]).arg(&image).arg(&text));

    let thumbs = count_files(&archive.path().join("data").join("thumb"));
    assert_eq!(thumbs, 1, "only the image gets a thumbnail");
}

#[test]
fn no_thumb_defers_generation_to_scan() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let image = src.path().join("photo.png");
    write_png(&image, 320, 240);

    run(d2mz(archive.path())
        .args(["ingest", "--no-thumb"])
        .arg(&image));
    assert_eq!(count_files(&archive.path().join("data").join("thumb")), 0);

    // `scan --thumb` backfills from the local blob store.
    run(d2mz(archive.path()).args(["scan", "--thumb"]));
    assert_eq!(count_files(&archive.path().join("data").join("thumb")), 1);
}

#[test]
fn image_thumbnail_is_generated_and_cached() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("photo.png");
    write_png(&file, 800, 600);

    run(d2mz(archive.path()).args(["ingest"]).arg(&file));

    let first = stdout(&run(d2mz(archive.path())
        .args(["thumb", "--print"])
        .arg(&file)));
    let thumb = Path::new(first.trim());
    assert!(thumb.exists(), "thumbnail not created at {first}");
    assert_eq!(thumb.extension().unwrap(), "webp");

    let modified = fs::metadata(thumb).unwrap().modified().unwrap();

    // A second call must reuse the cached file, not regenerate it.
    let second = stdout(&run(d2mz(archive.path())
        .args(["thumb", "--print"])
        .arg(&file)));
    assert_eq!(first.trim(), second.trim());
    let modified_again = fs::metadata(thumb).unwrap().modified().unwrap();
    assert_eq!(modified, modified_again, "thumbnail was regenerated");
}

#[test]
fn long_output_reports_dimensions() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("photo.png");
    write_png(&file, 1024, 256);

    run(d2mz(archive.path()).args(["ingest"]).arg(&file));
    let output = run(d2mz(archive.path()).args(["thumb", "-l"]).arg(&file));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("webp"), "{stderr}");
    // The longest edge is capped at the thumbnail size.
    assert!(stderr.contains("256"), "{stderr}");
}

#[test]
fn non_media_falls_back_to_the_blob() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("notes.txt");
    fs::write(&file, "hello\n").unwrap();

    run(d2mz(archive.path()).args(["ingest"]).arg(&file));
    let printed = stdout(&run(d2mz(archive.path())
        .args(["thumb", "--print"])
        .arg(&file)));
    assert!(printed.contains("store"), "{printed}");
}

#[test]
fn thumb_requires_an_archived_source() {
    let archive = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let file = src.path().join("photo.png");
    write_png(&file, 32, 32);

    // Not ingested yet, so there is no blob hash to key on.
    let output = d2mz(archive.path())
        .args(["thumb", "--print"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not archived") || stderr.contains("ingest"),
        "{stderr}"
    );
}

/// Keep the tempdir alive in helpers that take `&TempDir`.
#[allow(dead_code)]
fn keep(dir: &TempDir) -> &Path {
    dir.path()
}

/// Count files below `dir`, treating a missing directory as empty.
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
