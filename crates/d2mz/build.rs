//! Embeds the build's git revision into the binary.
//!
//! The CLI reports `0.8.0+<short-sha>` so it is obvious which working tree a
//! binary came from. When git is unavailable (for example a source tarball
//! build) the hash is simply omitted.

use std::process::Command;

fn main() {
    let revision = git_short_hash();
    match revision {
        Some(hash) => println!("cargo:rustc-env=D2MZ_GIT_SHA=+{hash}"),
        None => println!("cargo:rustc-env=D2MZ_GIT_SHA="),
    }

    // Rebuild when the checked-out revision changes.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
}

/// The short commit hash of the working tree, if it is a git checkout.
fn git_short_hash() -> Option<String> {
    if let Ok(sha) = std::env::var("D2MZ_GIT_SHA") {
        let sha = sha.trim().to_string();
        if !sha.is_empty() {
            return Some(sha);
        }
    }

    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if hash.is_empty() { None } else { Some(hash) }
}
