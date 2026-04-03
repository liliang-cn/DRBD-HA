pub mod generator;
pub mod models;

use anyhow::{Context, Result};
pub use generator::{generate_combined_files, generate_ts};
use models::ResourceAgent;
use quick_xml::de::from_str;
use std::fs;
use std::path::Path;
use std::process::Command;

fn controller_proxy_from_env() -> Option<(String, u16, String)> {
    let host = std::env::var("DRBD_HA_REMOTE_EXEC_HOST").ok()?;
    let port = std::env::var("DRBD_HA_REMOTE_EXEC_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(22);
    let user = std::env::var("DRBD_HA_REMOTE_EXEC_USER").unwrap_or_else(|_| "root".to_string());
    Some((host, port, user))
}

fn shell_escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn run_proxy_command(command: &str) -> Result<String> {
    let (host, port, user) =
        controller_proxy_from_env().context("proxy execution requested but no proxy configured")?;
    let target = format!("{}@{}", user, host);
    let remote_cmd = if user == "root" {
        format!("sh -lc '{}'", shell_escape_single_quotes(command))
    } else {
        format!("sudo -n sh -lc '{}'", shell_escape_single_quotes(command))
    };

    let output = Command::new("ssh")
        .arg("-p")
        .arg(port.to_string())
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
        .output()?;

    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr));
    }

    String::from_utf8(output.stdout).context("Invalid UTF-8 output")
}

pub fn get_agent_metadata(agent_path: &Path) -> Result<(ResourceAgent, String)> {
    if controller_proxy_from_env().is_some() {
        let command = format!(
            "OCF_ROOT=/usr/lib/ocf '{}' meta-data",
            shell_escape_single_quotes(&agent_path.to_string_lossy())
        );
        let xml_content = run_proxy_command(&command)?;
        let ra: ResourceAgent = from_str(&xml_content).context("Failed to parse XML")?;
        return Ok((ra, xml_content));
    }

    // Run the agent with meta-data argument
    let output = Command::new(agent_path)
        .arg("meta-data")
        .env("OCF_ROOT", "/usr/lib/ocf")
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "Agent returned error status: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let xml_content = String::from_utf8(output.stdout).context("Invalid UTF-8 output")?;
    let ra: ResourceAgent = from_str(&xml_content).context("Failed to parse XML")?;

    Ok((ra, xml_content))
}

pub fn get_agent_metadata_with_provider(
    agent_path: &Path,
    provider: &str,
) -> Result<(ResourceAgent, String)> {
    let (mut ra, xml_content) = get_agent_metadata(agent_path)?;
    ra.provider = provider.to_string();
    Ok((ra, xml_content))
}

pub fn list_agents(ocf_root: &Path) -> Result<Vec<(String, String)>> {
    let mut agents = Vec::new();
    let resource_d = ocf_root.join("resource.d");

    if controller_proxy_from_env().is_some() {
        let command = format!(
            "find '{}' -mindepth 2 -maxdepth 2 -type f -executable -printf '%P\n'",
            shell_escape_single_quotes(&resource_d.to_string_lossy())
        );
        let output = run_proxy_command(&command)?;

        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some((provider, agent)) = trimmed.split_once('/') {
                agents.push((provider.to_string(), agent.to_string()));
            }
        }

        return Ok(agents);
    }

    if !resource_d.exists() {
        return Ok(agents);
    }

    let entries = match fs::read_dir(&resource_d) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to read directory {}: {}", resource_d.display(), e);
            return Ok(agents);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();

        if path.is_dir() {
            let provider_name = match path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };

            let agent_entries = match fs::read_dir(&path) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for agent_entry in agent_entries {
                let agent_entry = match agent_entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let agent_path = agent_entry.path();

                if agent_path.is_file() {
                    // Use metadata call that doesn't fail the whole loop
                    if let Ok(metadata) = fs::metadata(&agent_path) {
                        use std::os::unix::fs::PermissionsExt;
                        if metadata.permissions().mode() & 0o111 != 0 {
                            let agent_name = match agent_path.file_name() {
                                Some(n) => n.to_string_lossy().to_string(),
                                None => continue,
                            };
                            agents.push((provider_name.clone(), agent_name));
                        }
                    }
                }
            }
        }
    }
    Ok(agents)
}
