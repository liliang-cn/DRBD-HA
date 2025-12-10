//! Transaction manager for distributed operations
//!
//! Provides a transaction mechanism with rollback capabilities for
//! operations that span multiple nodes.

use crate::core::{run_shell_command, SshCredential, SshManager};
use crate::error::{AppError, AppResult};
use std::sync::Arc;
use tracing::{error, info, warn};

/// A single operation that can be executed and rolled back
#[derive(Debug, Clone)]
pub struct Operation {
    /// Human-readable description
    pub description: String,
    /// Target node (None for local)
    pub target: Option<NodeTarget>,
    /// Command to execute
    pub command: String,
    /// Command to rollback (if operation succeeds but transaction fails)
    pub rollback_command: Option<String>,
    /// Whether failure of this operation should fail the transaction
    pub critical: bool,
}

/// Target node for remote operations
#[derive(Debug, Clone)]
pub struct NodeTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub credential: SshCredential,
}

/// Result of executing an operation
#[derive(Debug)]
pub struct OperationResult {
    pub operation: Operation,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Transaction state
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionState {
    Pending,
    Running,
    Committed,
    RolledBack,
    Failed,
}

/// Transaction for executing distributed operations with rollback support
pub struct Transaction {
    id: String,
    state: TransactionState,
    operations: Vec<Operation>,
    executed: Vec<OperationResult>,
    ssh_manager: Arc<SshManager>,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(ssh_manager: Arc<SshManager>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            state: TransactionState::Pending,
            operations: Vec::new(),
            executed: Vec::new(),
            ssh_manager,
        }
    }

    /// Get transaction ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get current state
    pub fn state(&self) -> &TransactionState {
        &self.state
    }

    /// Add a local operation
    pub fn add_local(&mut self, description: &str, command: &str) -> &mut Self {
        self.operations.push(Operation {
            description: description.to_string(),
            target: None,
            command: command.to_string(),
            rollback_command: None,
            critical: true,
        });
        self
    }

    /// Add a local operation with rollback
    pub fn add_local_with_rollback(
        &mut self,
        description: &str,
        command: &str,
        rollback: &str,
    ) -> &mut Self {
        self.operations.push(Operation {
            description: description.to_string(),
            target: None,
            command: command.to_string(),
            rollback_command: Some(rollback.to_string()),
            critical: true,
        });
        self
    }

    /// Add a remote operation
    pub fn add_remote(
        &mut self,
        description: &str,
        target: NodeTarget,
        command: &str,
    ) -> &mut Self {
        self.operations.push(Operation {
            description: description.to_string(),
            target: Some(target),
            command: command.to_string(),
            rollback_command: None,
            critical: true,
        });
        self
    }

    /// Add a remote operation with rollback
    pub fn add_remote_with_rollback(
        &mut self,
        description: &str,
        target: NodeTarget,
        command: &str,
        rollback: &str,
    ) -> &mut Self {
        self.operations.push(Operation {
            description: description.to_string(),
            target: Some(target),
            command: command.to_string(),
            rollback_command: Some(rollback.to_string()),
            critical: true,
        });
        self
    }

    /// Add a non-critical operation (failure won't abort transaction)
    pub fn add_non_critical(&mut self, description: &str, command: &str) -> &mut Self {
        self.operations.push(Operation {
            description: description.to_string(),
            target: None,
            command: command.to_string(),
            rollback_command: None,
            critical: false,
        });
        self
    }

    /// Execute a single operation
    async fn execute_operation(&self, op: &Operation) -> OperationResult {
        let result = if let Some(target) = &op.target {
            // Remote execution
            self.ssh_manager
                .execute(
                    &target.host,
                    target.port,
                    &target.user,
                    &target.credential,
                    &op.command,
                )
                .await
        } else {
            // Local execution
            run_shell_command(&op.command, &op.description).await
        };

        match result {
            Ok(output) => {
                if output.success() {
                    OperationResult {
                        operation: op.clone(),
                        success: true,
                        output: output.stdout,
                        error: None,
                    }
                } else {
                    OperationResult {
                        operation: op.clone(),
                        success: false,
                        output: output.stdout,
                        error: Some(output.stderr),
                    }
                }
            }
            Err(e) => OperationResult {
                operation: op.clone(),
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            },
        }
    }

    /// Execute the transaction
    pub async fn execute(&mut self) -> AppResult<()> {
        if self.state != TransactionState::Pending {
            return Err(AppError::Transaction(format!(
                "Transaction {} is not pending (state: {:?})",
                self.id, self.state
            )));
        }

        self.state = TransactionState::Running;
        info!("Starting transaction {}", self.id);

        // Clone operations to avoid borrow conflict
        let operations = self.operations.clone();
        let ops_len = operations.len();

        for (i, op) in operations.iter().enumerate() {
            info!(
                "Transaction {}: Executing step {}/{}: {}",
                self.id,
                i + 1,
                ops_len,
                op.description
            );

            let result = self.execute_operation(op).await;
            let op_description = op.description.clone();
            let op_critical = op.critical;

            if !result.success {
                if op_critical {
                    error!(
                        "Transaction {}: Critical operation failed: {} - {:?}",
                        self.id, op_description, result.error
                    );
                    self.executed.push(result);
                    self.state = TransactionState::Failed;

                    // Attempt rollback
                    if let Err(e) = self.rollback().await {
                        error!("Transaction {}: Rollback failed: {}", self.id, e);
                    }

                    return Err(AppError::Transaction(format!(
                        "Operation '{}' failed",
                        op_description
                    )));
                } else {
                    warn!(
                        "Transaction {}: Non-critical operation failed: {} - {:?}",
                        self.id, op_description, result.error
                    );
                }
            }

            self.executed.push(result);
        }

        self.state = TransactionState::Committed;
        info!("Transaction {} committed successfully", self.id);
        Ok(())
    }

    /// Rollback executed operations
    async fn rollback(&mut self) -> AppResult<()> {
        info!("Transaction {}: Starting rollback", self.id);

        // Rollback in reverse order
        for result in self.executed.iter().rev() {
            if result.success {
                if let Some(rollback_cmd) = &result.operation.rollback_command {
                    info!(
                        "Transaction {}: Rolling back: {}",
                        self.id, result.operation.description
                    );

                    let rollback_op = Operation {
                        description: format!("Rollback: {}", result.operation.description),
                        target: result.operation.target.clone(),
                        command: rollback_cmd.clone(),
                        rollback_command: None,
                        critical: false,
                    };

                    let rollback_result = self.execute_operation(&rollback_op).await;
                    if !rollback_result.success {
                        warn!(
                            "Transaction {}: Rollback operation failed: {:?}",
                            self.id, rollback_result.error
                        );
                    }
                }
            }
        }

        self.state = TransactionState::RolledBack;
        info!("Transaction {}: Rollback complete", self.id);
        Ok(())
    }

    /// Get executed operation results
    pub fn results(&self) -> &[OperationResult] {
        &self.executed
    }
}

/// Transaction builder for common DRBD operations
pub struct DrbdTransactionBuilder {
    ssh_manager: Arc<SshManager>,
}

impl DrbdTransactionBuilder {
    pub fn new(ssh_manager: Arc<SshManager>) -> Self {
        Self { ssh_manager }
    }

    /// Build transaction for creating a new DRBD resource
    pub fn create_resource(
        &self,
        resource_name: &str,
        config_content: &str,
        nodes: Vec<NodeTarget>,
    ) -> Transaction {
        let mut tx = Transaction::new(self.ssh_manager.clone());
        let config_path = format!("/etc/drbd.d/{}.res", resource_name);

        // Write config to all nodes
        for target in &nodes {
            // Use base64 to safely transfer content
            let encoded = base64_encode(config_content);
            let write_cmd = format!(
                "echo '{}' | base64 -d > '{}'",
                encoded,
                config_path.replace('\'', "'\\''")
            );
            let delete_cmd = format!("rm -f '{}'", config_path.replace('\'', "'\\''"));

            tx.add_remote_with_rollback(
                &format!("Write config to {}", target.host),
                target.clone(),
                &write_cmd,
                &delete_cmd,
            );
        }

        // Create metadata on all nodes
        for target in &nodes {
            tx.add_remote(
                &format!("Create metadata on {}", target.host),
                target.clone(),
                &format!("yes yes | drbdadm create-md {}", resource_name),
            );
        }

        // Bring up resource on all nodes
        for target in &nodes {
            let up_cmd = format!("drbdadm up {}", resource_name);
            let down_cmd = format!("drbdadm down {}", resource_name);
            tx.add_remote_with_rollback(
                &format!("Bring up resource on {}", target.host),
                target.clone(),
                &up_cmd,
                &down_cmd,
            );
        }

        tx
    }

    /// Build transaction for deleting a DRBD resource
    pub fn delete_resource(&self, resource_name: &str, nodes: Vec<NodeTarget>) -> Transaction {
        let mut tx = Transaction::new(self.ssh_manager.clone());
        let config_path = format!("/etc/drbd.d/{}.res", resource_name);

        // Down resource on all nodes
        for target in &nodes {
            tx.add_remote(
                &format!("Bring down resource on {}", target.host),
                target.clone(),
                &format!("drbdadm down {} 2>/dev/null || true", resource_name),
            );
        }

        // Remove config from all nodes
        for target in &nodes {
            tx.add_remote(
                &format!("Remove config from {}", target.host),
                target.clone(),
                &format!("rm -f '{}'", config_path),
            );
        }

        tx
    }
}

/// Simple base64 encoding
fn base64_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SshConfig;

    #[tokio::test]
    async fn test_transaction_state() {
        let ssh_config = SshConfig::default();
        let ssh_manager = Arc::new(SshManager::new(ssh_config));
        let tx = Transaction::new(ssh_manager);

        assert_eq!(*tx.state(), TransactionState::Pending);
        assert!(!tx.id().is_empty());
    }

    #[tokio::test]
    async fn test_transaction_add_operations() {
        let ssh_config = SshConfig::default();
        let ssh_manager = Arc::new(SshManager::new(ssh_config));
        let mut tx = Transaction::new(ssh_manager);

        tx.add_local("Test op 1", "echo hello")
            .add_local_with_rollback("Test op 2", "echo world", "echo rollback");

        assert_eq!(tx.operations.len(), 2);
        assert!(tx.operations[0].rollback_command.is_none());
        assert!(tx.operations[1].rollback_command.is_some());
    }
}
