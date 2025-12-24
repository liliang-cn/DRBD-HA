use super::StorageProvider;
#[cfg(not(test))]
use crate::core::shell_cmd::run_shell_command;
use crate::core::{CommandOutput, SshCredential, SshManager};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::sync::Arc;

// Test-only mocking framework for execute_command
#[cfg(test)]
mod mock_executor {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    // A global mock for `execute_command`
    // Use VecDeque for ordered outputs and Mutex for thread safety
    static MOCK_COMMAND_OUTPUTS: OnceLock<Mutex<Option<VecDeque<CommandOutput>>>> = OnceLock::new();

    fn get_mock_queue() -> &'static Mutex<Option<VecDeque<CommandOutput>>> {
        MOCK_COMMAND_OUTPUTS.get_or_init(|| Mutex::new(None))
    }

    // Clear mock outputs
    pub fn clear_mock_outputs() {
        let mut guard = get_mock_queue().lock().unwrap();
        *guard = None;
    }

    // Set the mock outputs for subsequent calls
    pub fn set_mock_command_outputs(outputs: Vec<CommandOutput>) {
        let mut guard = get_mock_queue().lock().unwrap();
        *guard = Some(outputs.into_iter().collect());
    }

    // This function will be called by LvmProvider::execute_command in test builds
    pub async fn mock_execute_command(command: &str, _description: &str) -> Result<CommandOutput> {
        let mut guard = get_mock_queue().lock().unwrap();
        if let Some(queue) = guard.as_mut() {
            if let Some(output) = queue.pop_front() {
                Ok(output)
            } else {
                eprintln!("Mock queue is empty for command: {}", command);
                Err(anyhow::anyhow!(
                    "Mock queue is empty for command: {}",
                    command
                ))
            }
        } else {
            eprintln!("Mock not set for execute_command. Command: {}", command);
            // Fallback to real execution in non-test or if explicit real behavior is desired
            Err(anyhow::anyhow!(
                "Mock not set for execute_command. Command: {}",
                command
            ))
        }
    }
}

pub struct LvmProvider {
    pub vg_name: String,
    pub thin_pool_name: Option<String>, // If set, use thin pool for volume creation
    // Optional SSH client for remote execution
    pub ssh_manager: Option<Arc<SshManager>>,
    pub ssh_target: Option<(String, u16, String, SshCredential)>, // host, port, user, credential
}

impl LvmProvider {
    pub fn new_local(vg_name: String) -> Self {
        LvmProvider {
            vg_name,
            thin_pool_name: Some("thinpool".to_string()), // Default to thin pool
            ssh_manager: None,
            ssh_target: None,
        }
    }

    pub fn new_local_with_thin_pool(vg_name: String, thin_pool_name: String) -> Self {
        LvmProvider {
            vg_name,
            thin_pool_name: Some(thin_pool_name),
            ssh_manager: None,
            ssh_target: None,
        }
    }

    pub fn new_remote(
        vg_name: String,
        ssh_manager: Arc<SshManager>,
        host: String,
        port: u16,
        user: String,
        credential: SshCredential,
    ) -> Self {
        LvmProvider {
            vg_name,
            thin_pool_name: Some("thinpool".to_string()), // Default to thin pool
            ssh_manager: Some(ssh_manager),
            ssh_target: Some((host, port, user, credential)),
        }
    }

    pub fn new_remote_with_thin_pool(
        vg_name: String,
        thin_pool_name: String,
        ssh_manager: Arc<SshManager>,
        host: String,
        port: u16,
        user: String,
        credential: SshCredential,
    ) -> Self {
        LvmProvider {
            vg_name,
            thin_pool_name: Some(thin_pool_name),
            ssh_manager: Some(ssh_manager),
            ssh_target: Some((host, port, user, credential)),
        }
    }

    // Conditional compilation for execute_command
    #[cfg(not(test))]
    async fn execute_command(&self, command: &str, description: &str) -> Result<CommandOutput> {
        if let (Some(manager), Some((host, port, user, credential))) =
            (&self.ssh_manager, &self.ssh_target)
        {
            manager
                .execute(host, *port, user, credential, command)
                .await
                .map_err(|e| anyhow::anyhow!("Remote LVM command failed on {}: {}", host, e))
        } else {
            run_shell_command(command, description)
                .await
                .map_err(|e| anyhow::anyhow!("Local LVM command failed: {}", e))
        }
    }

    #[cfg(test)]
    async fn execute_command(&self, command: &str, description: &str) -> Result<CommandOutput> {
        mock_executor::mock_execute_command(command, description).await
    }
}

#[async_trait]
impl StorageProvider for LvmProvider {
    async fn init_pool(&self, disk: &str) -> Result<()> {
        let command = format!("vgcreate {} {}", self.vg_name, disk);
        let output = self
            .execute_command(
                &command,
                &format!(
                    "Initialize LVM volume group '{}' on disk '{}'",
                    self.vg_name, disk
                ),
            )
            .await?;

        if !output.success() {
            bail!("Failed to initialize LVM pool: {}", output.stderr);
        }
        Ok(())
    }

    async fn create_volume(&self, vol_name: &str, size_gb: u64) -> Result<String> {
        // Use thin pool if configured, otherwise create thick volume
        let command = if let Some(ref thin_pool) = self.thin_pool_name {
            format!(
                "lvcreate -V {}G --type thin -n {} --thinpool {} {} --yes",
                size_gb, vol_name, thin_pool, self.vg_name
            )
        } else {
            format!(
                "lvcreate -L {}G -n {} {} --yes",
                size_gb, vol_name, self.vg_name
            )
        };

        let description = if self.thin_pool_name.is_some() {
            format!(
                "Create LVM thin logical volume '{}' of {}GB in VG '{}/{}'",
                vol_name,
                size_gb,
                self.vg_name,
                self.thin_pool_name.as_ref().unwrap_or(&"".to_string())
            )
        } else {
            format!(
                "Create LVM logical volume '{}' of {}GB in VG '{}'",
                vol_name, size_gb, self.vg_name
            )
        };

        let output = self.execute_command(&command, &description).await?;

        if !output.success() {
            bail!("Failed to create LVM volume: {}", output.stderr);
        }
        Ok(format!("/dev/{}/{}", self.vg_name, vol_name))
    }

    async fn delete_volume(&self, vol_name: &str) -> Result<()> {
        let command = format!("lvremove -f /dev/{}/{}", self.vg_name, vol_name);
        let output = self
            .execute_command(
                &command,
                &format!(
                    "Remove LVM logical volume '{}' from VG '{}'",
                    vol_name, self.vg_name
                ),
            )
            .await?;

        if !output.success() {
            bail!("Failed to delete LVM volume: {}", output.stderr);
        }
        Ok(())
    }

    async fn resize_volume(&self, vol_name: &str, new_size_gb: u64) -> Result<()> {
        let command = format!(
            "lvextend -L {}G /dev/{}/{}",
            new_size_gb, self.vg_name, vol_name
        );
        let output = self
            .execute_command(
                &command,
                &format!(
                    "Resize LVM logical volume '{}' to {}GB in VG '{}'",
                    vol_name, new_size_gb, self.vg_name
                ),
            )
            .await?;

        if !output.success() {
            bail!("Failed to resize LVM volume: {}", output.stderr);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::mock_executor::*;
    use super::*;
    use crate::core::shell_cmd::CommandOutput; // Ensure CommandOutput is visible
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_local_init_pool_success() {
        clear_mock_outputs();
        let provider = LvmProvider::new_local("test_vg".to_string());

        set_mock_command_outputs(vec![CommandOutput {
            stdout: "vgcreate output".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        }]);

        let result = provider.init_pool("/dev/sdb").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_local_init_pool_failure() {
        clear_mock_outputs();
        let provider = LvmProvider::new_local("test_vg".to_string());

        set_mock_command_outputs(vec![CommandOutput {
            stdout: "".to_string(),
            stderr: "vgcreate error".to_string(),
            exit_code: 1,
        }]);

        let result = provider.init_pool("/dev/sdb").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to initialize LVM pool"));
    }

    #[tokio::test]
    #[serial]
    async fn test_local_create_volume_success() {
        clear_mock_outputs();
        let provider = LvmProvider::new_local("test_vg".to_string());

        set_mock_command_outputs(vec![CommandOutput {
            stdout: "lvcreate output".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        }]);

        let result = provider.create_volume("test_lv", 10).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "/dev/test_vg/test_lv");
    }

    #[tokio::test]
    #[serial]
    async fn test_local_delete_volume_success() {
        clear_mock_outputs();
        let provider = LvmProvider::new_local("test_vg".to_string());

        set_mock_command_outputs(vec![CommandOutput {
            stdout: "lvremove output".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        }]);

        let result = provider.delete_volume("test_lv").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn test_local_resize_volume_success() {
        clear_mock_outputs();
        let provider = LvmProvider::new_local("test_vg".to_string());

        set_mock_command_outputs(vec![CommandOutput {
            stdout: "lvextend output".to_string(),
            stderr: "".to_string(),
            exit_code: 0,
        }]);

        let result = provider.resize_volume("test_lv", 20).await;
        assert!(result.is_ok());
    }
}
