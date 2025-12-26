use crate::error::DrbdResult;
use crate::models::DrbdResourceStatus;

/// Trait for executing commands on remote nodes
/// This allows the backend to provide SSH implementation while drbd-utils defines the logic
pub trait RemoteExecutor: Send + Sync {
    /// Execute a command on a remote node and return the output
    fn execute(&self, ip: &str, port: u16, user: &str, command: &str) -> impl std::future::Future<Output = DrbdResult<CommandOutput>> + Send;
}

/// Output from a remote command execution
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Remote DRBD query helper
pub struct RemoteDrbdQuery<E: RemoteExecutor> {
    executor: E,
    ssh_port: u16,
    ssh_user: String,
}

impl<E: RemoteExecutor> RemoteDrbdQuery<E> {
    pub fn new(executor: E, ssh_port: u16, ssh_user: &str) -> Self {
        Self {
            executor,
            ssh_port,
            ssh_user: ssh_user.to_string(),
        }
    }

    /// Get DRBD resource status from a remote node
    pub async fn get_resource_status(
        &self,
        remote_ip: &str,
        resource_name: &str,
    ) -> DrbdResult<Option<DrbdResourceStatus>> {
        let command = format!("drbdsetup status {} --json 2>/dev/null", resource_name);
        let sudo_command = format!("sudo {}", command);

        let output = self
            .executor
            .execute(remote_ip, self.ssh_port, &self.ssh_user, &sudo_command)
            .await?;

        if !output.stdout.is_empty() {
            match crate::parse_drbd_status(&output.stdout) {
                Ok(resources) => {
                    if let Some(resource) = resources.iter().find(|r| r.name == resource_name) {
                        return Ok(Some(crate::parser::convert_resource_status(resource)));
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse DRBD status from {}: {}", remote_ip, e);
                }
            }
        }

        Ok(None)
    }

    /// Get drbd-reactor status from a remote node
    pub async fn get_reactor_status(
        &self,
        remote_ip: &str,
        profile_name: &str,
    ) -> DrbdResult<Option<String>> {
        let command = format!("drbd-reactorctl status {}", profile_name);
        let sudo_command = format!("sudo {}", command);

        let output = self
            .executor
            .execute(remote_ip, self.ssh_port, &self.ssh_user, &sudo_command)
            .await?;

        Ok(Some(output.stdout))
    }

    /// Check if a systemd service is enabled on a remote node
    pub async fn is_service_enabled(
        &self,
        remote_ip: &str,
        service_name: &str,
    ) -> DrbdResult<bool> {
        let command = format!("systemctl is-enabled {}", service_name);
        let sudo_command = format!("sudo {}", command);

        let output = self
            .executor
            .execute(remote_ip, self.ssh_port, &self.ssh_user, &sudo_command)
            .await?;

        // systemctl is-enabled returns 0 if enabled, non-zero otherwise
        Ok(output.exit_code == 0)
    }
}

/// DNS resolution utility
pub fn resolve_hostname_to_ip(hostname: &str) -> Option<String> {
    use std::net::ToSocketAddrs;

    // Try to resolve hostname with a dummy port
    let addr = format!("{}:0", hostname);

    // Try to resolve using DNS
    match addr.to_socket_addrs() {
        Ok(mut addrs) => {
            if let Some(socket_addr) = addrs.next() {
                let ip = socket_addr.ip();
                // Only return IPv4 addresses
                if ip.is_ipv4() {
                    return Some(ip.to_string());
                }
            }
            None
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_localhost() {
        let ip = resolve_hostname_to_ip("localhost");
        assert!(ip.is_some());
        // localhost should resolve to 127.0.0.1
        assert_eq!(ip.unwrap(), "127.0.0.1");
    }
}
