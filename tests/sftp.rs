//! Integration tests against a real SFTP server (system sshd).
//!
//! Gated behind the `test-support` feature and skipped when `/usr/bin/sshd`
//! is absent. Set `D2MZ_SSHD_BIN` to point at a different sshd.

#![cfg(feature = "test-support")]

mod support;

use std::collections::BTreeMap;

use d2mz::backend::Backends;
use d2mz::config::{BackendConfig, Config};
use d2mz::uri::Uri;
use opendal::Operator;

use support::SftpServer;

fn sftp_backend(server: &SftpServer) -> BackendConfig {
    let mut options = BTreeMap::new();
    options.insert(
        "endpoint".to_string(),
        toml::Value::String(server.endpoint()),
    );
    options.insert(
        "user".to_string(),
        toml::Value::String(server.user().to_string()),
    );
    options.insert(
        "key".to_string(),
        toml::Value::String(server.client_key().to_string_lossy().to_string()),
    );
    options.insert(
        "known_hosts_strategy".to_string(),
        toml::Value::String("accept".to_string()),
    );
    options.insert(
        "root".to_string(),
        toml::Value::String(server.data_dir().to_string_lossy().to_string()),
    );
    BackendConfig {
        name: "mac".to_string(),
        scheme: "sftp".to_string(),
        options,
    }
}

async fn resolve(backends: &mut Backends, uri: &str) -> Operator {
    let uri = Uri::parse(uri).expect("parse uri");
    backends.resolve(&uri).expect("resolve backend")
}

#[tokio::test]
async fn sftp_can_browse_and_read() {
    let Some(server) = SftpServer::start().expect("start sshd") else {
        return;
    };

    let mut backends = Backends::new(Config::default());
    backends.add_backend(sftp_backend(&server));
    let operator = resolve(&mut backends, "mz://mac").await;
    assert_eq!(operator.info().scheme(), "sftp");

    let entries = operator
        .list_with("docs/")
        .recursive(false)
        .await
        .expect("list docs");
    let paths: Vec<&str> = entries.iter().map(|entry| entry.path()).collect();
    assert!(
        paths.iter().any(|path| path.starts_with("docs/report.txt")),
        "{paths:?}"
    );

    let content = operator
        .read("docs/report.txt")
        .await
        .expect("read report")
        .to_vec();
    assert_eq!(String::from_utf8(content).unwrap(), "quarterly report\n");
}

#[tokio::test]
async fn sftp_lists_recursively() {
    let Some(server) = SftpServer::start().expect("start sshd") else {
        return;
    };

    let mut backends = Backends::new(Config::default());
    backends.add_backend(sftp_backend(&server));
    let operator = resolve(&mut backends, "mz://mac").await;

    let entries = operator
        .list_with("")
        .recursive(true)
        .await
        .expect("list recursively");
    let names: Vec<String> = entries
        .iter()
        .map(|entry| entry.path().trim_end_matches('/').to_string())
        .collect();
    assert!(
        names.iter().any(|name| name.ends_with("docs/report.txt")),
        "{names:?}"
    );
    assert!(
        names.iter().any(|name| name.ends_with("media/photo.jpg")),
        "{names:?}"
    );
}
