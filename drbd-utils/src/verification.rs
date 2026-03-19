//! DRBD configuration verification utilities
//!
//! This module provides utilities to verify DRBD configuration and status
//! on local and remote nodes, including retry mechanisms for reliability.

use crate::error::DrbdResult;

/// Configuration for DRBD verification attempts
#[derive(Debug, Clone, Copy)]
pub struct VerificationConfig {
    /// Maximum number of verification attempts
    pub max_attempts: u32,
    /// Delay between attempts in seconds
    pub retry_delay_secs: u64,
    /// Whether to continue on verification failure
    pub continue_on_failure: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            retry_delay_secs: 2,
            continue_on_failure: true,
        }
    }
}

impl VerificationConfig {
    /// Create a config for quick verification
    pub fn quick() -> Self {
        Self {
            max_attempts: 1,
            retry_delay_secs: 0,
            continue_on_failure: true,
        }
    }

    /// Create a config for thorough verification
    pub fn thorough() -> Self {
        Self {
            max_attempts: 5,
            retry_delay_secs: 3,
            continue_on_failure: true,
        }
    }

    /// Create a config for strict verification (fail fast)
    pub fn strict() -> Self {
        Self {
            max_attempts: 2,
            retry_delay_secs: 1,
            continue_on_failure: false,
        }
    }
}

/// Result of DRBD verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether verification was successful
    pub success: bool,
    /// Number of attempts made
    pub attempts: u32,
    /// Last command output (if available)
    pub output: Option<String>,
    /// Last error message (if available)
    pub error: Option<String>,
    /// Verification details
    pub details: VerificationDetails,
}

/// Detailed verification information
#[derive(Debug, Clone)]
pub struct VerificationDetails {
    /// Whether the resource exists
    pub resource_exists: bool,
    /// Whether the resource is configured
    pub resource_configured: bool,
    /// Number of connected peers
    pub connected_peers: u32,
    /// Whether the resource is in a consistent state
    pub is_consistent: bool,
    /// Additional status information
    pub status_info: String,
}

/// DRBD verification utilities
pub struct DrbdVerifier;

impl DrbdVerifier {
    /// Verify DRBD configuration on a remote node via SSH
    ///
    /// # Arguments
    /// * `resource_name` - DRBD resource name to verify
    /// * `ssh_executor` - Async function that executes SSH commands and returns string output
    /// * `config` - Verification configuration
    ///
    /// # Returns
    /// `Ok(VerificationResult)` with verification details
    /// `Err(DrbdError)` if SSH execution fails
    pub async fn verify_remote_drbd_status<F, Fut>(
        resource_name: &str,
        ssh_executor: F,
        config: VerificationConfig,
    ) -> DrbdResult<VerificationResult>
    where
        F: FnOnce(String) -> Fut,
        Fut: std::future::Future<Output = Result<String, shell_cmd::error::ShellError>>,
    {
        let status_cmd = format!("drbdadm status {}", resource_name);

        match ssh_executor(status_cmd).await {
            Ok(output) => {
                // Parse DRBD status for detailed information
                let details = Self::parse_drbd_status_output(&output, resource_name);

                if details.resource_exists && details.resource_configured {
                    tracing::info!("✓ DRBD config verified for resource '{}'", resource_name);

                    Ok(VerificationResult {
                        success: true,
                        attempts: 1,
                        output: Some(output),
                        error: None,
                        details,
                    })
                } else {
                    tracing::warn!(
                        "⚠ DRBD resource '{}' not ready: {}",
                        resource_name,
                        details.status_info
                    );

                    if !config.continue_on_failure {
                        return Err(crate::error::DrbdError::Validation(format!(
                            "DRBD resource '{}' verification failed: {}",
                            resource_name, details.status_info
                        )));
                    }

                    Ok(VerificationResult {
                        success: false,
                        attempts: 1,
                        output: Some(output),
                        error: None,
                        details,
                    })
                }
            }
            Err(e) => {
                tracing::warn!(
                    "⚠ Failed to verify DRBD status for resource '{}': {}",
                    resource_name,
                    e
                );

                if !config.continue_on_failure {
                    return Err(crate::error::DrbdError::Validation(format!(
                        "DRBD status verification failed for resource '{}': {}",
                        resource_name, e
                    )));
                }

                Ok(VerificationResult {
                    success: false,
                    attempts: 1,
                    output: None,
                    error: Some(e.to_string()),
                    details: VerificationDetails {
                        resource_exists: false,
                        resource_configured: false,
                        connected_peers: 0,
                        is_consistent: false,
                        status_info: format!("SSH execution failed: {}", e),
                    },
                })
            }
        }
    }

    /// Parse DRBD status output to extract detailed information
    fn parse_drbd_status_output(output: &str, resource_name: &str) -> VerificationDetails {
        let mut details = VerificationDetails {
            resource_exists: false,
            resource_configured: false,
            connected_peers: 0,
            is_consistent: false,
            status_info: String::new(),
        };

        if output.is_empty() {
            details.status_info = "Empty status output".to_string();
            return details;
        }

        // Check if resource exists (has any output)
        details.resource_exists = !output.trim().is_empty();

        if !details.resource_exists {
            details.status_info = "Resource not found or no status output".to_string();
            return details;
        }

        // Special case: if output indicates resource was not found
        if output.contains("not found") || output.contains("No such resource") {
            details.resource_configured = false;
            details.status_info = "Resource not found".to_string();
            return details;
        }

        // Parse basic status information
        let lines: Vec<&str> = output.lines().collect();
        if lines.is_empty() {
            details.status_info = "No valid status lines".to_string();
            return details;
        }

        // First line should contain resource name and role
        if let Some(first_line) = lines.first() {
            if first_line.contains(resource_name) {
                details.resource_configured = true;

                // Count connected peers - peer lines typically start with whitespace and have node name
                for line in lines.iter().skip(1) {
                    if !line.trim().is_empty() && (line.starts_with("  ") || line.contains("role:"))
                    {
                        // Check if this line represents a peer connection
                        if line.contains("role:")
                            && !line.contains(&format!("{} role:", resource_name))
                        {
                            // This is a peer line (has role but is not the main resource line)
                            details.connected_peers += 1;

                            // Check for connection status
                            if line.contains("connection:Connected") {
                                details.is_consistent = true;
                            }
                        }
                    }
                }

                // Additional consistency checks
                details.is_consistent = details.is_consistent
                    && (output.contains("disk:UpToDate") || output.contains("peer-disk:UpToDate"));

                details.status_info = format!(
                    "Resource configured, {} peers, consistent: {}",
                    details.connected_peers, details.is_consistent
                );
            } else {
                details.status_info =
                    format!("Output doesn't contain resource '{}'", resource_name);
            }
        } else {
            details.status_info = "No status lines available".to_string();
        }

        details
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_config_default() {
        let config = VerificationConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.retry_delay_secs, 2);
        assert!(config.continue_on_failure);
    }

    #[test]
    fn test_verification_config_quick() {
        let config = VerificationConfig::quick();
        assert_eq!(config.max_attempts, 1);
        assert_eq!(config.retry_delay_secs, 0);
        assert!(config.continue_on_failure);
    }

    #[test]
    fn test_verification_config_strict() {
        let config = VerificationConfig::strict();
        assert_eq!(config.max_attempts, 2);
        assert_eq!(config.retry_delay_secs, 1);
        assert!(!config.continue_on_failure);
    }

    #[test]
    fn test_parse_drbd_status_empty() {
        let details = DrbdVerifier::parse_drbd_status_output("", "test");
        assert!(!details.resource_exists);
        assert!(!details.resource_configured);
        assert_eq!(details.connected_peers, 0);
        assert!(!details.is_consistent);
    }

    #[test]
    fn test_parse_drbd_status_basic() {
        let output = r#"mysql_data role:Primary
  disk:UpToDate open:yes
  node2 role:Secondary connection:Connected
    peer-disk:UpToDate"#;

        let details = DrbdVerifier::parse_drbd_status_output(output, "mysql_data");
        assert!(details.resource_exists);
        assert!(details.resource_configured);
        assert!(details.connected_peers > 0);
        assert!(details.is_consistent);
    }

    #[test]
    fn test_parse_drbd_status_not_configured() {
        let output = "Resource 'test_res' not found";

        let details = DrbdVerifier::parse_drbd_status_output(output, "test_res");
        assert!(details.resource_exists);
        assert!(!details.resource_configured);
        assert_eq!(details.connected_peers, 0);
        assert!(!details.is_consistent);
    }

    #[test]
    fn test_parse_drbd_status_no_peers() {
        let output = "test_res role:Secondary\n  disk:UpToDate open:no";

        let details = DrbdVerifier::parse_drbd_status_output(output, "test_res");
        assert!(details.resource_exists);
        assert!(details.resource_configured);
        assert_eq!(details.connected_peers, 0);
        assert!(!details.is_consistent); // No peers means not consistent
    }
}
