use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use quick_xml::de::from_str;
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

mod generator;
mod models;

use generator::{generate_combined_files, generate_ts};
use models::ResourceAgent;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert OCF resource agent XML metadata to JSON and TypeScript
    Convert {
        /// OCF root directory (overrides OCF_ROOT environment variable, default: /usr/lib/ocf)
        #[arg(short, long)]
        root: Option<PathBuf>,

        /// Output directory for JSON and TS files (default: current directory)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Save the raw XML metadata files
        #[arg(short, long, default_value_t = false)]
        save_xml: bool,

        /// Combine all TS into one file and all JSON into one file
        #[arg(short, long, default_value_t = false)]
        combine: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Convert {
            root,
            output,
            save_xml,
            combine,
        } => {
            let ocf_root = root
                .or_else(|| env::var("OCF_ROOT").ok().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("/usr/lib/ocf"));

            let output_dir = output.unwrap_or_else(|| PathBuf::from("."));
            fs::create_dir_all(&output_dir)?;

            let resource_d = ocf_root.join("resource.d");
            if !resource_d.exists() {
                anyhow::bail!("OCF resource directory not found: {}", resource_d.display());
            }

            println!("Scanning OCF agents in: {}", resource_d.display());
            scan_and_process(&resource_d, &output_dir, save_xml, combine)?;
        }
    }

    Ok(())
}

fn scan_and_process(
    resource_d: &Path,
    output_dir: &Path,
    save_xml: bool,
    combine: bool,
) -> Result<()> {
    let mut all_agents = Vec::new();
    let mut success_count = 0;
    let mut fail_count = 0;
    let mut failures: Vec<(String, String)> = Vec::new();

    // Iterate over providers (e.g., heartbeat, linbit, pacemaker)
    for entry in fs::read_dir(resource_d)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let provider_name = path.file_name().unwrap().to_string_lossy();
            let provider_output_dir = output_dir.join(&*provider_name);
            fs::create_dir_all(&provider_output_dir)?;

            // Iterate over agents in the provider directory
            for agent_entry in fs::read_dir(&path)? {
                let agent_entry = agent_entry?;
                let agent_path = agent_entry.path();

                // Check if it's a file and executable
                if agent_path.is_file() {
                    let metadata = fs::metadata(&agent_path)?;
                    let permissions = metadata.permissions();
                    if permissions.mode() & 0o111 != 0 {
                        // It's executable, try to get meta-data
                        match process_agent(&agent_path, &provider_output_dir, save_xml) {
                            Ok(agent) => {
                                success_count += 1;
                                if combine {
                                    all_agents.push(agent);
                                }
                            }
                            Err(e) => {
                                fail_count += 1;
                                let agent_name =
                                    agent_path.file_name().unwrap().to_string_lossy().to_string();
                                failures.push((agent_name.clone(), e.to_string()));
                                eprintln!("Failed to process agent {}: {}", agent_path.display(), e);
                            }
                        }
                    }
                }
            }
        }
    }

    if combine && !all_agents.is_empty() {
        generate_combined_files(&all_agents, output_dir)?;
    }

    println!("\n--- Processing Summary ---");
    println!("Total Agents Processed: {}", success_count + fail_count);
    println!("Successful: {}", success_count);
    println!("Failed: {}", fail_count);

    if !failures.is_empty() {
        println!("\n--- Failures ---");
        for (name, reason) in failures {
            println!("Agent: {}\n  Reason: {}\n", name, reason);
        }
    }

    Ok(())
}

fn process_agent(
    agent_path: &Path,
    output_dir: &Path,
    save_xml: bool,
) -> Result<ResourceAgent> {
    let agent_name = agent_path.file_name().unwrap().to_string_lossy();
    println!("Processing Agent: {}", agent_name);

    // Run the agent with meta-data argument
    let output = Command::new(agent_path)
        .arg("meta-data")
        .env("OCF_ROOT", "/usr/lib/ocf")
        .output()
        .context("Failed to execute agent with meta-data")?;

    if !output.status.success() {
        anyhow::bail!(
            "Agent returned error status: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let xml_content = String::from_utf8(output.stdout).context("Invalid UTF-8 output")?;

    // Optional: Save XML
    if save_xml {
        let xml_path = output_dir.join(format!("{}.xml", agent_name));
        let mut file = fs::File::create(&xml_path)?;
        file.write_all(xml_content.as_bytes())?;
        println!("  Saved XML: {}", xml_path.display());
    }

    // Parse XML
    let ra: ResourceAgent = from_str(&xml_content).context("Failed to parse XML")?;

    // Generate JSON
    let json_output_path = output_dir.join(format!("{}.json", agent_name));
    let json_file = fs::File::create(&json_output_path)?;
    serde_json::to_writer_pretty(json_file, &ra)?;
    println!("  Generated JSON: {}", json_output_path.display());

    // Generate TypeScript
    let ts_output_path = output_dir.join(format!("{}.ts", agent_name));
    let ts_content = generate_ts(&ra, &agent_name);
    let mut ts_file = fs::File::create(&ts_output_path)?;
    ts_file.write_all(ts_content.as_bytes())?;
    println!("  Generated TS: {}", ts_output_path.display());

    Ok(ra)
}
