//! Backend configuration for the MZ layer.
//!
//! `mz::Config` describes the named backends only. Application settings
//! (archive location, sync, ignore lists) live above this layer.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Name of the implicit backend used for bare paths.
pub const LOCAL_BACKEND: &str = "local";

/// Backend configuration: the named data sources MZ can address.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    /// Named backends, in declaration order.
    #[serde(default, rename = "backend")]
    pub backends: Vec<BackendConfig>,
}

/// A single named backend definition.
///
/// `name` and `scheme` are interpreted by MZ, every other key is passed
/// through to the matching OpenDAL service builder. Values are strings so
/// that the mapping stays predictable across services.
#[derive(Debug, Clone, Deserialize)]
pub struct BackendConfig {
    /// Backend name used in `mz://<name>/...` URIs.
    pub name: String,
    /// OpenDAL scheme, e.g. `fs`, `s3`, `sftp`.
    pub scheme: String,
    /// Scheme-specific options passed through to OpenDAL.
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
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?
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
    pub fn ensure_local_backend(&mut self) {
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

/// Expand a leading `~/` using the user's home directory.
pub fn expand_tilde(raw: &str) -> std::path::PathBuf {
    if let Some(rest) = raw.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    std::path::PathBuf::from(raw)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_implicit_local_backend() {
        let mut config = Config::default();
        config.ensure_local_backend();
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
        assert_eq!(config.backends.len(), 2);

        let r2 = config.backend("r2").unwrap();
        assert_eq!(r2.scheme, "s3");
        let opts = r2.opendal_options();
        assert_eq!(opts.get("bucket").unwrap(), "mybucket");
        assert_eq!(opts.get("endpoint").unwrap(), "https://example.invalid");
    }

    #[test]
    fn parses_sftp_backend() {
        let text = r#"
            [[backend]]
            name = "mac"
            scheme = "sftp"
            endpoint = "ssh://mac.local:22"
            user = "ada"
            key = "~/.ssh/id_ed25519"
            known_hosts_strategy = "accept"
            root = "/Users/ada"
        "#;
        let config: Config = toml::from_str(text).unwrap();
        let mac = config.backend("mac").unwrap();
        assert_eq!(mac.scheme, "sftp");
        let opts = mac.opendal_options();
        assert_eq!(opts.get("endpoint").unwrap(), "ssh://mac.local:22");
        assert_eq!(opts.get("user").unwrap(), "ada");
        assert_eq!(opts.get("known_hosts_strategy").unwrap(), "accept");
    }
}
