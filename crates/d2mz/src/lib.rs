//! The d2mz command-line application.
//!
//! This crate is the top layer: it owns the CLI, the archive, and the
//! presentation of results. Data access goes through the `mz` crate.

pub mod archive;
pub mod browse;
pub mod cli;
pub mod commands;
pub mod config;
pub mod output;

// Re-exported so callers keep using `d2mz::uri::Uri` and friends.
pub use mz::{backend, uri};

pub use mz::init_http;
