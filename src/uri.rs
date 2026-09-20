//! Parsing of d2mz URIs.
//!
//! A URI names a backend and a path inside it. Three spellings are accepted:
//!
//! * `mz://<backend>/<path>` — canonical form.
//! * `<backend>://<path>` — shorthand when the backend name is unambiguous,
//!   e.g. `local://etc/hosts`.
//! * a bare filesystem path — shorthand for the `local` backend, e.g.
//!   `/etc/hosts` or `./notes.txt`.

use std::fmt;

use anyhow::{Result, bail};

use crate::config::LOCAL_BACKEND;

/// A resolved backend name plus a backend-relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    backend: String,
    path: String,
}

impl Uri {
    /// Parse a URI from user input.
    pub fn parse(input: &str) -> Result<Uri> {
        if let Some((scheme, rest)) = input.split_once("://") {
            let (backend, path) = if scheme == "mz" {
                match rest.split_once('/') {
                    Some((authority, path)) => (authority, path),
                    None => (rest, ""),
                }
            } else {
                (scheme, rest)
            };
            if backend.is_empty() {
                bail!("missing backend name in {input:?}");
            }
            return Ok(Uri {
                backend: backend.to_string(),
                path: normalize_path(path),
            });
        }

        Ok(Uri {
            backend: LOCAL_BACKEND.to_string(),
            path: normalize_path(input),
        })
    }

    /// Name of the target backend.
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Backend-relative path (never begins with `/`).
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Whether the path is empty, i.e. the backend root.
    pub fn is_root(&self) -> bool {
        self.path.is_empty()
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mz://{}/{}", self.backend, self.path)
    }
}

/// Strip leading `/` and `./` so paths are relative to the backend root.
fn normalize_path(path: &str) -> String {
    let mut path = path;
    loop {
        if let Some(rest) = path.strip_prefix("./") {
            path = rest;
        } else if let Some(rest) = path.strip_prefix('/') {
            path = rest;
        } else {
            break;
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_absolute_path_is_local() {
        let uri = Uri::parse("/etc/hosts").unwrap();
        assert_eq!(uri.backend(), LOCAL_BACKEND);
        assert_eq!(uri.path(), "etc/hosts");
    }

    #[test]
    fn bare_relative_path_is_local() {
        let uri = Uri::parse("./notes/today.txt").unwrap();
        assert_eq!(uri.backend(), LOCAL_BACKEND);
        assert_eq!(uri.path(), "notes/today.txt");
    }

    #[test]
    fn canonical_mz_uri() {
        let uri = Uri::parse("mz://r2/backups/2026.tar").unwrap();
        assert_eq!(uri.backend(), "r2");
        assert_eq!(uri.path(), "backups/2026.tar");
        assert_eq!(uri.to_string(), "mz://r2/backups/2026.tar");
    }

    #[test]
    fn scheme_shorthand() {
        let uri = Uri::parse("local://etc/hosts").unwrap();
        assert_eq!(uri.backend(), "local");
        assert_eq!(uri.path(), "etc/hosts");
    }

    #[test]
    fn backend_without_path_is_root() {
        let uri = Uri::parse("mz://r2").unwrap();
        assert!(uri.is_root());
        let uri = Uri::parse("mz://r2/").unwrap();
        assert!(uri.is_root());
    }

    #[test]
    fn empty_backend_is_rejected() {
        assert!(Uri::parse("mz:///etc").is_err());
    }
}
