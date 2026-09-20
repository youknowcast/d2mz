//! Application configuration for d2mz.
//!
//! Backend definitions are owned by the MZ layer (`mz::Config`); this type
//! adds the application concerns (archive location, sync, ignore lists) and
//! reads the same TOML file. The on-disk format is unchanged.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

pub use mz::config::expand_tilde;

/// Application settings plus the MZ backend definitions.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// Directory backing the content-addressed archive.
    #[serde(default = "default_archive_dir", deserialize_with = "de_pathbuf")]
    pub archive_dir: PathBuf,

    /// The single shared "main" database, e.g. `mz://rfs/d2mz/main.db`.
    ///
    /// There is exactly one main database; it is the source of truth that
    /// `sync` merges with. Leaving it unset keeps d2mz purely local.
    #[serde(default)]
    pub main: Option<String>,

    /// Sync automatically after commands that change the index.
    ///
    /// Off by default. When enabled alongside `main`, an `ingest`, `tag`,
    /// `forget` or `scan` pushes its result without a separate `d2mz sync`,
    /// so a forgotten push stops being a failure mode.
    #[serde(default)]
    pub auto_sync: bool,

    /// Glob patterns excluded from `ingest`.
    ///
    /// Defaults to the usual filesystem noise. Set it to `[]` to ingest
    /// everything, or list your own patterns to replace the defaults.
    #[serde(default = "default_ignore")]
    pub ignore: Vec<String>,

    /// MZ backend definitions, pulled from the same document.
    #[serde(flatten)]
    pub mz: mz::Config,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            archive_dir: default_archive_dir(),
            main: None,
            auto_sync: false,
            ignore: default_ignore(),
            mz: mz::Config::default(),
        }
    }
}

impl AppConfig {
    /// Resolve the configuration file path.
    pub fn path() -> Result<PathBuf> {
        mz::Config::path()
    }

    /// Load the configuration from [`AppConfig::path`].
    pub fn load() -> Result<AppConfig> {
        AppConfig::load_from(&AppConfig::path()?)
    }

    /// Load the configuration from an explicit path.
    ///
    /// A missing file is not an error; the default configuration is used.
    pub fn load_from(path: &Path) -> Result<AppConfig> {
        let mut config: AppConfig = if path.exists() {
            let text = fs::read_to_string(path)
                .with_context(|| format!("reading config {}", path.display()))?;
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?
        } else {
            AppConfig::default()
        };
        config.mz.ensure_local_backend();
        Ok(config)
    }

    /// Look up a backend by name.
    pub fn backend(&self, name: &str) -> Option<&mz::BackendConfig> {
        self.mz.backend(name)
    }
}

fn default_archive_dir() -> PathBuf {
    match dirs::data_dir() {
        Some(base) => base.join("d2mz"),
        None => PathBuf::from(".d2mz"),
    }
}

/// Filesystem noise that should never end up in the archive.
fn default_ignore() -> Vec<String> {
    [
        ".DS_Store",
        "Thumbs.db",
        "desktop.ini",
        ".git",
        ".git/**",
        "**/.git/**",
        "*.tmp",
        "*.swp",
        "*~",
    ]
    .iter()
    .map(|pattern| pattern.to_string())
    .collect()
}

/// Deserialize a string into a [`PathBuf`], expanding a leading `~`.
fn de_pathbuf<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(expand_tilde(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_injects_local_backend() {
        let mut config = AppConfig::default();
        config.mz.ensure_local_backend();
        let local = config.backend(mz::LOCAL_BACKEND).expect("local backend");
        assert_eq!(local.scheme, "fs");
    }

    #[test]
    fn parses_app_and_backend_settings_together() {
        let text = r#"
            archive_dir = "/tmp/d2mz-archive"
            main = "mz://rfs/d2mz/main.db"
            auto_sync = true
            ignore = []

            [[backend]]
            name = "rfs"
            scheme = "s3"
            bucket = "mybucket"
        "#;
        let config: AppConfig = toml::from_str(text).unwrap();
        assert_eq!(config.archive_dir, PathBuf::from("/tmp/d2mz-archive"));
        assert_eq!(config.main.as_deref(), Some("mz://rfs/d2mz/main.db"));
        assert!(config.auto_sync);
        assert!(config.ignore.is_empty());
        assert_eq!(config.backend("rfs").unwrap().scheme, "s3");
    }

    #[test]
    fn ignore_defaults_and_overrides() {
        let config = AppConfig::default();
        assert!(config.ignore.iter().any(|p| p == ".DS_Store"));

        let config: AppConfig = toml::from_str("ignore = []").unwrap();
        assert!(config.ignore.is_empty());
    }

    #[test]
    fn expands_tilde() {
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expand_tilde("~/x"), home.join("x"));
        }
        assert_eq!(expand_tilde("/abs/x"), PathBuf::from("/abs/x"));
    }
}
