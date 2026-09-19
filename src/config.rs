//! Configuration loading for d2mz.
//!
//! The configuration describes named backends (the "MZ" virtual data
//! sources) plus the location of the local archive. It is read from
//! `$D2MZ_CONFIG` when set, otherwise from `<config-dir>/d2mz/config.toml`.
//! A missing file simply yields the default configuration.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Name of the implicit backend used for bare paths.
pub const LOCAL_BACKEND: &str = "local";

/// Top-level configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Directory backing the content-addressed archive.
    #[serde(default = "default_archive_dir", deserialize_with = "de_pathbuf")]
    pub archive_dir: PathBuf,

    /// Named backends, in declaration order.
    #[serde(default, rename = "backend")]
    pub backends: Vec<BackendConfig>,
}

/// A single named backend definition.
///
/// `name` and `scheme` are interpreted by d2mz, every other key is passed
/// through to the matching OpenDAL service builder. Values are strings so
/// that the mapping stays predictable across services.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub scheme: String,
    #[serde(flatten)]
    pub options: BTreeMap<String, toml::Value>,
}

impl BackendConfig {
    /// Options converted to the string map OpenDAL expects.
    pub fn opendal_options(&self) -> BTreeMap<String, String> {
        self.options
            .iter()
            .filter_map(|(k, v)| value_to_string(v).map(|s| (k.clone(), s)))
            .collect()
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut config = Config {
            archive_dir: default_archive_dir(),
            backends: Vec::new(),
        };
        config.ensure_local_backend();
        config
    }
}

impl Config {
    /// Resolve the configuration file path.
    pub fn path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("D2MZ_CONFIG") {
            return Ok(PathBuf::from(path));
        }
        let base = dirs::config_dir().context("cannot determine config directory")?;
        Ok(base.join("d2mz").join("config.toml"))
    }

    /// Load the configuration from [`Config::path`].
    pub fn load() -> Result<Config> {
        Config::load_from(&Config::path()?)
    }

    /// Load the configuration from an explicit path.
    ///
    /// A missing file is not an error; the default configuration is used.
    pub fn load_from(path: &Path) -> Result<Config> {
        let mut config: Config = if path.exists() {
            let text = fs::read_to_string(path)
                .with_context(|| format!("reading config {}", path.display()))?;
            toml::from_str(&text)
                .with_context(|| format!("parsing config {}", path.display()))?
        } else {
            Config::default()
        };
        config.ensure_local_backend();
        Ok(config)
    }

    /// Look up a backend by name.
    pub fn backend(&self, name: &str) -> Option<&BackendConfig> {
        self.backends.iter().find(|b| b.name == name)
    }

    /// Guarantee a `local` filesystem backend exists.
    fn ensure_local_backend(&mut self) {
        if self.backend(LOCAL_BACKEND).is_none() {
            let mut options = BTreeMap::new();
            options.insert("root".to_string(), toml::Value::String("/".to_string()));
            self.backends.insert(
                0,
                BackendConfig {
                    name: LOCAL_BACKEND.to_string(),
                    scheme: "fs".to_string(),
                    options,
                },
            );
        }
    }
}

fn default_archive_dir() -> PathBuf {
    match dirs::data_dir() {
        Some(base) => base.join("d2mz"),
        None => PathBuf::from(".d2mz"),
    }
}

fn value_to_string(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Float(f) => Some(f.to_string()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Deserialize a string into a [`PathBuf`], expanding a leading `~`.
fn de_pathbuf<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(expand_tilde(&raw))
}

/// Expand a leading `~/` using the user's home directory.
pub fn expand_tilde(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_implicit_local_backend() {
        let mut config = Config::default();
        assert_eq!(config.backends.len(), 1);
        config.ensure_local_backend();
        assert_eq!(config.backends.len(), 1, "ensure must be idempotent");
        let local = config.backend(LOCAL_BACKEND).expect("local backend");
        assert_eq!(local.scheme, "fs");
        assert_eq!(local.opendal_options().get("root").unwrap(), "/");
    }

    #[test]
    fn parses_backends_and_options() {
        let text = r#"
            archive_dir = "/tmp/d2mz-archive"

            [[backend]]
            name = "local"
            scheme = "fs"
            root = "/home"

            [[backend]]
            name = "r2"
            scheme = "s3"
            bucket = "mybucket"
            endpoint = "https://example.invalid"
            region = "auto"
        "#;
        let config: Config = toml::from_str(text).unwrap();
        assert_eq!(config.archive_dir, PathBuf::from("/tmp/d2mz-archive"));
        assert_eq!(config.backends.len(), 2);

        let r2 = config.backend("r2").unwrap();
        assert_eq!(r2.scheme, "s3");
        let opts = r2.opendal_options();
        assert_eq!(opts.get("bucket").unwrap(), "mybucket");
        assert_eq!(opts.get("endpoint").unwrap(), "https://example.invalid");
    }

    #[test]
    fn load_from_missing_file_uses_defaults() {
        let config = Config::load_from(Path::new("/nonexistent/d2mz.toml")).unwrap();
        assert!(config.backend(LOCAL_BACKEND).is_some());
    }

    #[test]
    fn expands_tilde() {
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expand_tilde("~/x"), home.join("x"));
        }
        assert_eq!(expand_tilde("/abs/x"), PathBuf::from("/abs/x"));
    }
}
