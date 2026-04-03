use std::process::Stdio;
use tokio::process::Command;

const REMOTE_EXEC_HOST_ENV: &str = "DRBD_HA_REMOTE_EXEC_HOST";
const REMOTE_EXEC_PORT_ENV: &str = "DRBD_HA_REMOTE_EXEC_PORT";
const REMOTE_EXEC_USER_ENV: &str = "DRBD_HA_REMOTE_EXEC_USER";

#[derive(Debug, Clone)]
struct ProxyTarget {
    host: String,
    port: u16,
    user: String,
}

fn proxy_target_from_env() -> Option<ProxyTarget> {
    let host = std::env::var(REMOTE_EXEC_HOST_ENV).ok()?;
    let port = std::env::var(REMOTE_EXEC_PORT_ENV)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(22);
    let user = std::env::var(REMOTE_EXEC_USER_ENV).unwrap_or_else(|_| "root".to_string());

    Some(ProxyTarget { host, port, user })
}

fn shell_escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn shell_quote_arg(value: &str) -> String {
    format!("'{}'", shell_escape_single_quotes(value))
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Command failed: {0}")]
    Command(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub async fn run_command(cmd: &str, args: &[&str]) -> Result<String> {
    let output = if let Some(proxy) = proxy_target_from_env() {
        let target = format!("{}@{}", proxy.user, proxy.host);
        let full_cmd = std::iter::once(shell_quote_arg(cmd))
            .chain(args.iter().map(|arg| shell_quote_arg(arg)))
            .collect::<Vec<_>>()
            .join(" ");
        let remote_cmd = if proxy.user == "root" {
            format!("sh -lc {}", shell_quote_arg(&full_cmd))
        } else {
            format!("sudo -n sh -lc {}", shell_quote_arg(&full_cmd))
        };

        Command::new("ssh")
            .arg("-p")
            .arg(proxy.port.to_string())
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("UserKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(target)
            .arg(remote_cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?
    } else {
        Command::new(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?
    };

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(Error::Command(format!(
            "Command '{} {}' failed: {}",
            cmd,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}
