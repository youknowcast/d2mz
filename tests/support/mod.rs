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
// Each integration test binary compiles this whole module but uses only part
// of it, so unused-item warnings here are expected.
#![allow(dead_code)]

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

/// A running sshd exposing an SFTP subsystem over localhost.
///
/// Uses the system `sshd` and `ssh-keygen`, so it is skipped unless
/// `D2MZ_SSHD_BIN` is set (defaults to `/usr/bin/sshd` when that exists).
pub struct SftpServer {
    child: Child,
    port: u16,
    root: TempDir,
    user: String,
}

impl SftpServer {
    /// Start an SFTP server, returning `None` when sshd is unavailable.
    pub fn start() -> Result<Option<SftpServer>> {
        let sshd = std::env::var_os("D2MZ_SSHD_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr/bin/sshd"));
        if !sshd.exists() {
            eprintln!("sshd not found; skipping SFTP-backed test");
            return Ok(None);
        }

        let root = tempfile::tempdir().context("creating SFTP root")?;
        let data = root.path().join("data");
        std::fs::create_dir_all(data.join("docs"))?;
        std::fs::create_dir_all(data.join("media"))?;
        std::fs::write(data.join("docs/report.txt"), "quarterly report\n")?;
        std::fs::write(data.join("media/photo.jpg"), "vacation photo\n")?;

        let host_key = root.path().join("host_key");
        let client_key = root.path().join("client_key");
        keygen(&host_key)?;
        keygen(&client_key)?;

        let ssh_dir = root.path().join(".ssh");
        std::fs::create_dir_all(&ssh_dir)?;
        std::fs::copy(
            client_key.with_extension("pub"),
            ssh_dir.join("authorized_keys"),
        )?;
        set_mode(&ssh_dir, 0o700)?;
        set_mode(&ssh_dir.join("authorized_keys"), 0o600)?;

        let port = free_port()?;
        let user = std::env::var("USER").unwrap_or_else(|_| "root".to_string());
        let config = root.path().join("sshd_config");
        std::fs::write(
            &config,
            format!(
                "Port {port}\n\
                 ListenAddress 127.0.0.1\n\
                 HostKey {host}\n\
                 PidFile {pid}\n\
                 AuthorizedKeysFile {auth}\n\
                 PasswordAuthentication no\n\
                 KbdInteractiveAuthentication no\n\
                 UsePAM no\n\
                 StrictModes no\n\
                 Subsystem sftp internal-sftp\n\
                 LogLevel ERROR\n",
                host = host_key.display(),
                pid = root.path().join("sshd.pid").display(),
                auth = ssh_dir.join("authorized_keys").display(),
            ),
        )?;

        let child = Command::new(&sshd)
            .arg("-f")
            .arg(&config)
            .arg("-D")
            .arg("-e")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawning {}", sshd.display()))?;

        let server = SftpServer {
            child,
            port,
            root,
            user,
        };
        server.wait_until_ready()?;
        Ok(Some(server))
    }

    /// Endpoint in the form OpenDAL expects.
    pub fn endpoint(&self) -> String {
        format!("ssh://127.0.0.1:{}", self.port)
    }

    /// Remote user to authenticate as.
    pub fn user(&self) -> &str {
        &self.user
    }

    /// Path to the private key accepted by the server.
    pub fn client_key(&self) -> PathBuf {
        self.root.path().join("client_key")
    }

    /// Absolute path of the served data directory.
    pub fn data_dir(&self) -> PathBuf {
        self.root.path().join("data")
    }

    fn wait_until_ready(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if std::net::TcpStream::connect(("127.0.0.1", self.port)).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        bail!("sshd did not become ready on port {}", self.port)
    }
}

impl Drop for SftpServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn keygen(path: &std::path::Path) -> Result<()> {
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(path)
        .status()
        .context("running ssh-keygen")?;
    if !status.success() {
        bail!("ssh-keygen failed for {}", path.display());
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &std::path::Path, _mode: u32) -> Result<()> {
    Ok(())
}
