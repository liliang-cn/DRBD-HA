//! Adapter that runs DRBD-HA remote node operations through the `dispatch`
//! library (crate `dispatch-rs`).
//!
//! This is the seam for migrating off the hand-rolled `ssh-cmd` / `shell-cmd`
//! layer: `dispatch::write` creates parent directories and surfaces failures,
//! which removes the whole class of "No such file or directory" / silently
//! swallowed-error bugs the old code was prone to.

use crate::error::{AppError, AppResult};
use crate::models::cluster::Node;
use dispatch::{Config, Dispatch, HostResult};

/// `ssh://user@ip:port` destination for a node (system ssh honors ~/.ssh/config
/// for keys/known_hosts/proxy on top of this).
fn target(node: &Node) -> String {
    format!("ssh://{}@{}:{}", node.ssh_user, node.ip, node.ssh_port)
}

/// A `dispatch` client tuned for one node: non-root users go through `sudo -n`.
fn client_for(node: &Node) -> AppResult<Dispatch> {
    let cfg = Config {
        sudo: node.ssh_user != "root",
        ..Default::default()
    };
    Dispatch::new(cfg).map_err(|e| AppError::Ssh(e.to_string()))
}

/// Remote operations on a single [`Node`], backed by `dispatch`.
pub struct DispatchClient;

impl DispatchClient {
    /// Run `cmd` on `node`, returning the per-host result (stdout/stderr/exit).
    pub async fn exec(node: &Node, cmd: &str) -> AppResult<HostResult> {
        let client = client_for(node)?;
        let result = client
            .exec([target(node)], cmd)
            .run()
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))?;
        result
            .hosts
            .into_values()
            .next()
            .ok_or_else(|| AppError::Ssh("dispatch returned no host result".into()))
    }

    /// Write `content` to `path` on `node`, creating parent directories first.
    pub async fn write(
        node: &Node,
        content: &[u8],
        path: &str,
        mode: Option<u32>,
    ) -> AppResult<()> {
        let client = client_for(node)?;
        let mut builder = client.write([target(node)], content.to_vec(), path);
        if let Some(m) = mode {
            builder = builder.mode(m);
        }
        let result = builder
            .run()
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))?;
        if result.all_success() {
            Ok(())
        } else {
            let detail = result
                .hosts
                .values()
                .find_map(|h| h.error.clone())
                .unwrap_or_default();
            Err(AppError::Ssh(format!("write to {path} failed: {detail}")))
        }
    }

    /// Read `path` from `node`, returning its bytes.
    pub async fn read(node: &Node, path: &str) -> AppResult<Vec<u8>> {
        let client = client_for(node)?;
        let result = client
            .read([target(node)], path)
            .run()
            .await
            .map_err(|e| AppError::Ssh(e.to_string()))?;
        let host = result
            .hosts
            .into_values()
            .next()
            .ok_or_else(|| AppError::Ssh("dispatch returned no host result".into()))?;
        if host.success {
            Ok(host.content)
        } else {
            Err(AppError::Ssh(format!(
                "read {path} failed: {}",
                host.error.unwrap_or_default()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live round-trip against a real node. Opt-in:
    /// `DISPATCH_TEST_NODE=root@192.168.123.214 cargo test -p drbd-ha dispatch_client -- --nocapture`
    #[tokio::test]
    async fn live_write_read_roundtrip() {
        let Ok(spec) = std::env::var("DISPATCH_TEST_NODE") else {
            eprintln!("skipping: set DISPATCH_TEST_NODE=user@ip[:port]");
            return;
        };
        let (user, rest) = spec.split_once('@').expect("expected user@ip");
        let (ip, port) = match rest.split_once(':') {
            Some((ip, p)) => (ip.to_string(), p.parse().unwrap()),
            None => (rest.to_string(), 22u16),
        };
        let node = Node {
            id: "test".into(),
            hostname: "test".into(),
            ip,
            ssh_port: port,
            ssh_user: user.into(),
            is_local: false,
            status: Default::default(),
            status_message: None,
            last_seen: None,
        };

        let path = "/tmp/drbd-ha-dispatch/poc/x.conf";
        let content = b"# drbd-ha via dispatch\n";
        DispatchClient::write(&node, content, path, Some(0o600))
            .await
            .expect("write");
        let got = DispatchClient::read(&node, path).await.expect("read");
        assert_eq!(got, content);

        let r = DispatchClient::exec(&node, "rm -rf /tmp/drbd-ha-dispatch; echo cleaned")
            .await
            .expect("exec");
        assert!(r.success);
        assert_eq!(r.stdout.trim(), "cleaned");
    }
}
