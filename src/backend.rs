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
        if let Some(operator) = self.cache.get(uri.backend()) {
            return Ok(operator.clone());
        }

        let backend = self.config.backend(uri.backend()).with_context(|| {
            let known = self.names().join(", ");
            format!(
                "unknown backend {:?}; configured backends: {known}",
                uri.backend()
            )
        })?;
        let operator = build_operator(backend)?;
        self.cache
            .insert(uri.backend().to_string(), operator.clone());
        Ok(operator)
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
    let options = backend.opendal_options();
    Operator::via_iter(&backend.scheme, options).with_context(|| {
        format!(
            "creating backend {:?} (scheme {:?})",
            backend.name, backend.scheme
        )
    })
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
}
