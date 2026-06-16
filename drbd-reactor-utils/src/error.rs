use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone)]
struct ProxyTarget {
    host: String,
    port: u16,
    user: String,
}

fn proxy_target_from_env() -> Option<ProxyTarget> {
    dispatch_config::command_proxy().map(|p| ProxyTarget {
        host: p.host,
        port: p.port,
        user: p.user,
    })
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
    let (stdout, stderr, success) = if let Some(proxy) = proxy_target_from_env() {
        // Remote: keep login shell + sudo exactly as before, run via dispatch.
        let full_cmd = std::iter::once(shell_quote_arg(cmd))
            .chain(args.iter().map(|arg| shell_quote_arg(arg)))
            .collect::<Vec<_>>()
            .join(" ");
        let remote_cmd = if proxy.user == "root" {
            format!("sh -lc {}", shell_quote_arg(&full_cmd))
        } else {
            format!("sudo -n sh -lc {}", shell_quote_arg(&full_cmd))
        };
        let cfg = dispatch_config::default_dispatch_config();
        let client = dispatch::Dispatch::new(cfg)
            .map_err(|e| Error::Command(format!("dispatch init failed: {}", e)))?;
        let dest = format!("ssh://{}@{}:{}", proxy.user, proxy.host, proxy.port);
        let result = client
            .exec([dest], remote_cmd)
            .run()
            .await
            .map_err(|e| Error::Command(format!("dispatch exec failed: {}", e)))?;
        let hr = result
            .hosts
            .into_values()
            .next()
            .ok_or_else(|| Error::Command("dispatch returned no host result".into()))?;
        (hr.stdout, hr.stderr, hr.success)
    } else {
        let output = Command::new(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        (
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
            output.status.success(),
        )
    };

    if success {
        Ok(stdout)
    } else {
        Err(Error::Command(format!(
            "Command '{} {}' failed: {}",
            cmd,
            args.join(" "),
            stderr
        )))
    }
}
