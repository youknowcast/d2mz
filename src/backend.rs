//! The "MZ" data-access layer.
//!
//! [`Backends`] turns a [`Config`] plus a [`Uri`] into an OpenDAL
//! [`Operator`]. Everything above this module works with the uniform
//! operator API and never has to know whether the bytes come from a local
//! file, S3, or any other service.

use std::collections::HashMap;

use anyhow::{Context, Result};
use opendal::Operator;

use crate::config::{BackendConfig, Config};
use crate::uri::Uri;

/// Resolves URIs to OpenDAL operators, caching one operator per backend.
pub struct Backends {
    config: Config,
    cache: HashMap<String, Operator>,
}

impl Backends {
    /// Build a resolver from an already loaded configuration.
    pub fn new(config: Config) -> Backends {
        Backends {
            config,
            cache: HashMap::new(),
        }
    }

    /// Load the configuration and build a resolver.
    pub fn from_config() -> Result<Backends> {
        Ok(Backends::new(Config::load()?))
    }

    /// Resolve `uri` to an operator, building and caching it on first use.
    pub fn resolve(&mut self, uri: &Uri) -> Result<Operator> {
        Ok(self.resolve_with(uri, build_operator)?.0)
    }

    /// Like [`Backends::resolve`] but with a custom operator factory and
    /// reporting the resolved backend name.
    pub fn resolve_with<F>(&mut self, uri: &Uri, builder: F) -> Result<(Operator, String)>
    where
        F: FnOnce(&BackendConfig) -> Result<Operator>,
    {
        if let Some(operator) = self.cache.get(uri.backend()) {
            return Ok((operator.clone(), uri.backend().to_string()));
        }

        let backend = self.config.backend(uri.backend()).with_context(|| {
            let known = self.names().join(", ");
            format!(
                "unknown backend {:?}; configured backends: {known}",
                uri.backend()
            )
        })?;
        let operator = builder(backend)?;
        self.cache
            .insert(uri.backend().to_string(), operator.clone());
        Ok((operator, uri.backend().to_string()))
    }

    /// Register an additional backend definition at runtime.
    ///
    /// Used by the test suite to inject an S3 backend and by any future
    /// in-process sources. Existing caches are unaffected.
    pub fn add_backend(&mut self, backend: BackendConfig) {
        self.config.backends.push(backend);
    }

    /// Names of all configured backends, in declaration order.
    pub fn names(&self) -> Vec<&str> {
        self.config
            .backends
            .iter()
            .map(|b| b.name.as_str())
            .collect()
    }
}

fn build_operator(backend: &BackendConfig) -> Result<Operator> {
    let options = apply_scheme_defaults(backend);
    Operator::via_iter(&backend.scheme, options).with_context(|| {
        format!(
            "creating backend {:?} (scheme {:?})",
            backend.name, backend.scheme
        )
    })
}

/// Fill in options that are practically required but easy to forget.
///
/// For S3, non-AWS endpoints (RustFS, MinIO, R2, ...) need path-style
/// addressing and a disabled config/credential file lookup. Both can still
/// be overridden from the config file.
fn apply_scheme_defaults(backend: &BackendConfig) -> std::collections::BTreeMap<String, String> {
    let mut options = backend.opendal_options();
    if backend.scheme == "s3" {
        let endpoint = options.get("endpoint").map(String::as_str).unwrap_or("");
        let is_aws = endpoint.is_empty() || endpoint.contains("amazonaws.com");
        if !is_aws {
            options
                .entry("enable_virtual_host_style".to_string())
                .or_insert_with(|| "false".to_string());
            options
                .entry("disable_config_load".to_string())
                .or_insert_with(|| "true".to_string());
        }
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_implicit_local_backend() {
        let mut backends = Backends::new(Config::default());
        let uri = Uri::parse("/etc/hosts").unwrap();
        let operator = backends.resolve(&uri).unwrap();
        assert_eq!(operator.info().scheme(), "fs");
        assert_eq!(backends.names(), vec!["local"]);
    }

    #[test]
    fn unknown_backend_is_an_error() {
        let mut backends = Backends::new(Config::default());
        let uri = Uri::parse("mz://nope/x").unwrap();
        let error = backends.resolve(&uri).unwrap_err().to_string();
        assert!(error.contains("unknown backend"), "{error}");
    }

    fn s3_backend(endpoint: &str) -> BackendConfig {
        let mut options = std::collections::BTreeMap::new();
        options.insert("bucket".into(), toml::Value::String("b".into()));
        if !endpoint.is_empty() {
            options.insert("endpoint".into(), toml::Value::String(endpoint.into()));
        }
        BackendConfig {
            name: "s3".into(),
            scheme: "s3".into(),
            options,
        }
    }

    #[test]
    fn non_aws_endpoints_get_path_style_defaults() {
        let options = apply_scheme_defaults(&s3_backend("http://127.0.0.1:9000"));
        assert_eq!(options.get("enable_virtual_host_style").unwrap(), "false");
        assert_eq!(options.get("disable_config_load").unwrap(), "true");
    }

    #[test]
    fn aws_endpoints_keep_opendal_defaults() {
        let options = apply_scheme_defaults(&s3_backend("https://s3.us-east-1.amazonaws.com"));
        assert!(!options.contains_key("enable_virtual_host_style"));
        assert!(!options.contains_key("disable_config_load"));

        let options = apply_scheme_defaults(&s3_backend(""));
        assert!(!options.contains_key("disable_config_load"));
    }

    #[test]
    fn explicit_options_are_respected() {
        let mut backend = s3_backend("http://127.0.0.1:9000");
        backend.options.insert(
            "enable_virtual_host_style".into(),
            toml::Value::String("true".into()),
        );
        let options = apply_scheme_defaults(&backend);
        assert_eq!(options.get("enable_virtual_host_style").unwrap(), "true");
    }
}
