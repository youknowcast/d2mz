//! Integration tests against a real RustFS S3 endpoint.
//!
//! Gated behind the `test-support` feature and skipped unless
//! `D2MZ_RUSTFS_BIN` points at a RustFS binary. Set `D2MZ_RUSTFS_CLI` to also
//! let the harness seed objects; otherwise the test uploads through d2mz only.

#![cfg(feature = "test-support")]

mod support;

use std::collections::BTreeMap;
use std::process::Command;

use d2mz::backend::Backends;
use d2mz::config::{BackendConfig, Config};
use d2mz::uri::Uri;
use opendal::Operator;

use support::{ACCESS_KEY, REGION, RustfsServer, SECRET_KEY};

const BUCKET: &str = "d2mz-it";

/// Build an S3 backend definition pointing at `endpoint`.
fn s3_backend(endpoint: &str) -> BackendConfig {
    let mut options = BTreeMap::new();
    options.insert(
        "bucket".to_string(),
        toml::Value::String(BUCKET.to_string()),
    );
    options.insert(
        "endpoint".to_string(),
        toml::Value::String(endpoint.to_string()),
    );
    options.insert(
        "region".to_string(),
        toml::Value::String(REGION.to_string()),
    );
    options.insert(
        "access_key_id".to_string(),
        toml::Value::String(ACCESS_KEY.to_string()),
    );
    options.insert(
        "secret_access_key".to_string(),
        toml::Value::String(SECRET_KEY.to_string()),
    );
    BackendConfig {
        name: "rfs".to_string(),
        scheme: "s3".to_string(),
        options,
    }
}

/// Run the `rc` client from `D2MZ_RUSTFS_CLI` if available.
fn rc_cli() -> Option<std::path::PathBuf> {
    std::env::var_os("D2MZ_RUSTFS_CLI").map(std::path::PathBuf::from)
}

fn create_bucket(endpoint: &str) {
    let Some(rc) = rc_cli() else {
        return;
    };
    let alias = "d2mz-it";
    let status = Command::new(&rc)
        .args(["alias", "set", alias, endpoint, ACCESS_KEY, SECRET_KEY])
        .status()
        .expect("run rc alias set");
    assert!(status.success(), "rc alias set failed");
    let status = Command::new(&rc)
        .args(["bucket", "create", &format!("{alias}/{BUCKET}")])
        .status()
        .expect("run rc bucket create");
    assert!(status.success(), "rc bucket create failed");
}

fn put_object(_endpoint: &str, key: &str, body: &str) {
    let Some(rc) = rc_cli() else {
        return;
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let local = dir.path().join("payload");
    std::fs::write(&local, body).expect("write payload");
    let status = Command::new(&rc)
        .args(["object", "copy"])
        .arg(&local)
        .arg(format!("d2mz-it/{BUCKET}/{key}"))
        .status()
        .expect("run rc object copy");
    assert!(status.success(), "rc object copy failed");
}

async fn resolve(backends: &mut Backends, uri: &str) -> Operator {
    d2mz::init_http();
    let uri = Uri::parse(uri).expect("parse uri");
    backends.resolve(&uri).expect("resolve backend")
}

#[tokio::test]
async fn s3_browse_round_trip() {
    let Some(server) = RustfsServer::start().expect("start rustfs") else {
        return;
    };
    let endpoint = server.endpoint();
    create_bucket(&endpoint);
    put_object(&endpoint, "notes/hello.txt", "hello from rustfs");

    let mut backends = Backends::new(Config::default());
    backends.add_backend(s3_backend(&endpoint));

    let operator = resolve(&mut backends, "mz://rfs").await;
    assert_eq!(operator.info().scheme(), "s3");

    let entries = operator
        .list_with("notes/")
        .recursive(false)
        .await
        .expect("list notes");
    let expected = entries
        .iter()
        .map(|entry| entry.path().to_string())
        .collect::<Vec<_>>();
    assert_eq!(expected, vec![entry_path("notes/hello.txt")]);

    let content = operator
        .read("notes/hello.txt")
        .await
        .expect("read object")
        .to_vec();
    assert_eq!(String::from_utf8(content).unwrap(), "hello from rustfs");
}

/// OpenDAL reports file paths without a trailing slash.
fn entry_path(path: &str) -> String {
    path.trim_end_matches('/').to_string()
}

#[tokio::test]
async fn s3_read_back_what_we_write() {
    let Some(server) = RustfsServer::start().expect("start rustfs") else {
        return;
    };
    let endpoint = server.endpoint();
    create_bucket(&endpoint);

    let mut backends = Backends::new(Config::default());
    backends.add_backend(s3_backend(&endpoint));
    let operator = resolve(&mut backends, "mz://rfs").await;

    operator
        .write("generated/note.txt", "written by d2mz")
        .await
        .expect("write object");
    let content = operator
        .read("generated/note.txt")
        .await
        .expect("read object")
        .to_vec();
    assert_eq!(String::from_utf8(content).unwrap(), "written by d2mz");
}
