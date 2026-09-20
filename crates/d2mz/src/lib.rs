//! The d2mz command-line application.
//!
//! This crate is the top layer: it owns the CLI, the archive, and the
//! presentation of results. Data access goes through the `mz` crate.

pub mod browse;
pub mod cli;
pub mod commands;
pub mod config;
pub mod interactive;
pub mod output;

// Re-exported so callers keep using `d2mz::uri::Uri` and friends.
pub use mz::{backend, uri};

pub use mz::init_http;

/// The version string, including the build's git revision when known.
///
/// Reported as `0.8.0+abcd123`, so it is clear which working tree a binary
/// came from. The suffix is empty for non-git builds.
pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), env!("D2MZ_GIT_SHA"));
