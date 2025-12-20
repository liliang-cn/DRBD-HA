pub mod generator;
pub mod models;

pub use generator::{generate_combined_files, generate_ts};
use anyhow::{Context, Result};
use models::ResourceAgent;
use quick_xml::de::from_str;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn get_agent_metadata(agent_path: &Path) -> Result<ResourceAgent> {
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

    Ok(ra)
}

pub fn list_agents(ocf_root: &Path) -> Result<Vec<(String, String)>> {
    let mut agents = Vec::new();
    let resource_d = ocf_root.join("resource.d");

    if !resource_d.exists() {
        return Ok(agents);
    }

    for entry in fs::read_dir(resource_d)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let provider_name = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();

            for agent_entry in fs::read_dir(&path)? {
                let agent_entry = agent_entry?;
                let agent_path = agent_entry.path();

                if agent_path.is_file() {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = fs::metadata(&agent_path)?;
                    if metadata.permissions().mode() & 0o111 != 0 {
                        let agent_name = agent_path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .to_string();
                        agents.push((provider_name.clone(), agent_name));
                    }
                }
            }
        }
    }
    Ok(agents)
}
