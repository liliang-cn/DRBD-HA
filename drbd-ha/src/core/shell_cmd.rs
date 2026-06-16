use crate::error::{AppError, AppResult};
pub use shell_cmd::CommandOutput;

#[derive(Debug, Clone)]
pub struct CommandProxyConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
}

pub fn configure_command_proxy(config: Option<CommandProxyConfig>) {
    dispatch_config::set_command_proxy(config.map(|c| dispatch_config::CommandProxy {
        host: c.host,
        port: c.port,
        user: c.user,
    }));
}

pub fn current_command_proxy() -> Option<CommandProxyConfig> {
    dispatch_config::command_proxy().map(|c| CommandProxyConfig {
        host: c.host,
        port: c.port,
        user: c.user,
    })
}

fn shell_escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

async fn run_shell_command_via_proxy(
    proxy: &CommandProxyConfig,
    cmd_str: &str,
) -> AppResult<CommandOutput> {
    // Keep the login shell + sudo exactly as before; only the transport changes
    // from a raw `ssh` spawn to the `dispatch` library.
    let escaped_cmd = shell_escape_single_quotes(cmd_str);
    let remote_cmd = if proxy.user == "root" {
        format!("sh -lc '{}'", escaped_cmd)
    } else {
        format!("sudo -n sh -lc '{}'", escaped_cmd)
    };

    // sudo is already baked into remote_cmd above; don't double-wrap.
    let cfg = dispatch_config::default_dispatch_config();
    let client = dispatch::Dispatch::new(cfg)
        .map_err(|e| AppError::Internal(format!("dispatch init failed: {}", e)))?;
    let dest = format!("ssh://{}@{}:{}", proxy.user, proxy.host, proxy.port);
    let result = client.exec([dest], remote_cmd).run().await.map_err(|e| {
        AppError::Internal(format!("Failed to execute proxied shell command: {}", e))
    })?;
    let hr = result
        .hosts
        .into_values()
        .next()
        .ok_or_else(|| AppError::Internal("dispatch returned no host result".into()))?;

    Ok(CommandOutput {
        stdout: hr.stdout,
        stderr: hr.stderr,
        exit_code: hr.exit_code,
    })
}

/// Executes a shell command and returns its output.
///
/// # Arguments
/// * `cmd_str` - The command string to execute.
/// * `description` - A brief description of the command's purpose for logging.
pub async fn run_shell_command(cmd_str: &str, description: &str) -> AppResult<CommandOutput> {
    if let Some(proxy) = current_command_proxy() {
        if !description.is_empty() {
            tracing::debug!(
                "Executing proxied command via {}@{}:{}: '{}' ({})",
                proxy.user,
                proxy.host,
                proxy.port,
                cmd_str,
                description
            );
        }
        run_shell_command_via_proxy(&proxy, cmd_str).await
    } else {
        shell_cmd::run_shell_command(cmd_str, description)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))
    }
}
