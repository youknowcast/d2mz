//! A lease-based pessimistic lock over any backend.
//!
//! The remote "main" database may live on S3 or SFTP where POSIX locks do not
//! exist, so exclusion is built from a lock object that is created atomically
//! (`If-None-Match: *` on S3, rename on a filesystem). A holder records an
//! expiry; an expired lease may be taken over, which keeps a crashed client
//! from blocking everyone forever.

use anyhow::{Context, Result, bail};
use opendal::Operator;
use serde::{Deserialize, Serialize};

/// Default lease duration in seconds.
pub const DEFAULT_TTL_SECS: u64 = 300;

/// The contents written into a lock object.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Lease {
    /// Node that holds the lock.
    pub owner: String,
    /// Human-readable host, for diagnostics.
    pub host: String,
    /// Unix timestamp the lease was taken.
    pub acquired_at: i64,
    /// Unix timestamp after which the lease may be stolen.
    pub expires_at: i64,
}

impl Lease {
    /// Whether the lease is still valid at `now`.
    pub fn is_live(&self, now: i64) -> bool {
        self.expires_at > now
    }
}

/// A held lock; releasing it removes the lock object.
#[derive(Debug)]
pub struct LockGuard {
    operator: Operator,
    path: String,
    lease: Lease,
}

impl LockGuard {
    /// The lease that was acquired.
    pub fn lease(&self) -> &Lease {
        &self.lease
    }

    /// Release the lock, but only if this node still owns it.
    pub async fn release(self) -> Result<()> {
        let current = read_lease(&self.operator, &self.path).await?;
        match current {
            Some(lease) if lease.owner == self.lease.owner => {
                self.operator
                    .delete(&self.path)
                    .await
                    .with_context(|| format!("releasing lock {}", self.path))?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Acquire the lock at `path`, taking over an expired lease if needed.
pub async fn acquire(
    operator: &Operator,
    path: &str,
    node: &str,
    host: &str,
    ttl_secs: u64,
) -> Result<LockGuard> {
    let now = unix_now();
    let lease = Lease {
        owner: node.to_string(),
        host: host.to_string(),
        acquired_at: now,
        expires_at: now + ttl_secs as i64,
    };
    let body = serde_json::to_vec(&lease)?;

    // Optimistic create: succeeds only when no lock object exists.
    if operator
        .write_with(path, body.clone())
        .if_not_exists(true)
        .await
        .is_ok()
    {
        return Ok(LockGuard {
            operator: operator.clone(),
            path: path.to_string(),
            lease,
        });
    }

    // Someone holds it. Steal only if the lease has expired.
    if let Some(existing) = read_lease(operator, path).await? {
        if existing.is_live(now) {
            bail!(
                "lock {} is held by {} on {} until {}",
                path,
                existing.owner,
                existing.host,
                existing.expires_at
            );
        }
        operator
            .write(path, body)
            .await
            .with_context(|| format!("stealing expired lock {path}"))?;
        return Ok(LockGuard {
            operator: operator.clone(),
            path: path.to_string(),
            lease,
        });
    }

    // The object vanished between attempts, or the backend cannot express
    // `if_not_exists`; surface a clear error rather than proceeding unlocked.
    bail!("could not acquire lock {path}: it exists but is unreadable")
}

/// Read the lease currently stored at `path`, if any.
async fn read_lease(operator: &Operator, path: &str) -> Result<Option<Lease>> {
    match operator.read(path).await {
        Ok(buffer) => {
            let lease = serde_json::from_slice(&buffer.to_vec())
                .with_context(|| format!("parsing lock {path}"))?;
            Ok(Some(lease))
        }
        Err(error) if error.kind() == opendal::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading lock {path}")),
    }
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory() -> Operator {
        Operator::new(opendal::services::Memory::default()).unwrap()
    }

    #[test]
    fn lease_liveness() {
        let lease = Lease {
            owner: "a".into(),
            host: "h".into(),
            acquired_at: 100,
            expires_at: 200,
        };
        assert!(lease.is_live(150));
        assert!(!lease.is_live(200));
        assert!(!lease.is_live(250));
    }

    #[tokio::test]
    async fn acquire_then_release() {
        let op = memory();
        let guard = acquire(&op, "lock.json", "node-a", "host-a", 60)
            .await
            .unwrap();
        assert_eq!(guard.lease().owner, "node-a");
        guard.release().await.unwrap();

        // Now free, so it can be acquired again.
        let guard = acquire(&op, "lock.json", "node-b", "host-b", 60)
            .await
            .unwrap();
        assert_eq!(guard.lease().owner, "node-b");
    }

    #[tokio::test]
    async fn live_lock_blocks_others() {
        let op = memory();
        let _held = acquire(&op, "lock.json", "node-a", "host-a", 60)
            .await
            .unwrap();

        let error = acquire(&op, "lock.json", "node-b", "host-b", 60)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("is held by"), "{error}");
    }

    #[tokio::test]
    async fn expired_lock_can_be_stolen() {
        let op = memory();
        // Write a lease that is already expired.
        let stale = Lease {
            owner: "dead".into(),
            host: "gone".into(),
            acquired_at: 1,
            expires_at: 2,
        };
        op.write("lock.json", serde_json::to_vec(&stale).unwrap())
            .await
            .unwrap();

        let guard = acquire(&op, "lock.json", "node-b", "host-b", 60)
            .await
            .unwrap();
        assert_eq!(guard.lease().owner, "node-b");
    }
}
