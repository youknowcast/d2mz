//! Test support for spinning up a real RustFS S3 endpoint.
//!
//! RustFS is the Rust successor to MinIO. The binary is not vendored; set
//! `D2MZ_RUSTFS_BIN` to its path (and optionally `D2MZ_RUSTFS_CLI`) to enable
//! the S3 integration tests. When unset, the tests skip themselves.
//!
//! ```sh
//! curl -LO https://github.com/rustfs/rustfs/releases/download/1.0.0/rustfs-linux-x86_64-gnu-latest.zip
//! unzip rustfs-linux-x86_64-gnu-latest.zip
//! D2MZ_RUSTFS_BIN=$PWD/rustfs D2MZ_RUSTFS_CLI=$PWD/rc cargo test
//! ```
//!
//! The harness is behind `#[cfg(test)]`-style gating via the `test-support`
//! feature so that no test-only process management ships in the binary.

#![cfg(feature = "test-support")]

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tempfile::TempDir;

/// Access key used by the harness and expected in test configs.
pub const ACCESS_KEY: &str = "d2mz-test-access";
/// Secret key used by the harness and expected in test configs.
pub const SECRET_KEY: &str = "d2mz-test-secret";
/// Region configured on the RustFS instance.
pub const REGION: &str = "us-east-1";

/// A running RustFS server bound to an ephemeral localhost port.
pub struct RustfsServer {
    child: Child,
    port: u16,
    _data: TempDir,
}

impl RustfsServer {
    /// Start RustFS, returning `None` when no binary is configured.
    pub fn start() -> Result<Option<RustfsServer>> {
        let Some(bin) = std::env::var_os("D2MZ_RUSTFS_BIN") else {
            eprintln!("D2MZ_RUSTFS_BIN not set; skipping RustFS-backed test");
            return Ok(None);
        };
        let bin = PathBuf::from(bin);
        if !bin.exists() {
            bail!(
                "D2MZ_RUSTFS_BIN points at a missing file: {}",
                bin.display()
            );
        }

        let port = free_port()?;
        let data = tempfile::tempdir().context("creating RustFS data dir")?;

        let child = Command::new(&bin)
            .arg("server")
            .arg(data.path())
            .arg("--address")
            .arg(format!("127.0.0.1:{port}"))
            .arg("--access-key")
            .arg(ACCESS_KEY)
            .arg("--secret-key")
            .arg(SECRET_KEY)
            .arg("--region")
            .arg(REGION)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawning {}", bin.display()))?;

        let server = RustfsServer {
            child,
            port,
            _data: data,
        };
        server.wait_until_ready()?;
        Ok(Some(server))
    }

    /// Endpoint URL, e.g. `http://127.0.0.1:39321`.
    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Poll the S3 port until it accepts connections.
    fn wait_until_ready(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if std::net::TcpStream::connect(("127.0.0.1", self.port)).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        bail!("RustFS did not become ready on port {}", self.port)
    }
}

impl Drop for RustfsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Ask the OS for an unused localhost port.
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}
