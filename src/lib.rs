//! Library entry points shared by the binary and the test suite.

pub mod archive;
pub mod backend;
pub mod browse;
pub mod cli;
pub mod commands;
pub mod config;
pub mod output;
pub mod uri;

/// Install the process-wide HTTP transport and service registry.
///
/// The transport is installed lazily and only when an HTTP-based backend is
/// actually used, so local-only invocations never initialize TLS. Must be
/// called before constructing an S3 (or other HTTP) operator.
pub fn init_http() {
    install_crypto_provider();
    opendal_http_transport_reqwest::install_default();
}

/// Select ring as the process-wide rustls crypto provider.
///
/// rustls 0.23 has no default provider; this must happen before the first
/// TLS client is built. It is idempotent and safe to call more than once.
fn install_crypto_provider() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}
