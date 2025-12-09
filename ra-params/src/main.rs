use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use quick_xml::de::from_str;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ResourceAgent {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@version")]
    version: String,
    #[serde(default)]
    longdesc: LocalizedText,
    #[serde(default)]
    shortdesc: LocalizedText,
    #[serde(default)]
    parameters: Parameters,
    #[serde(default)]
    actions: Actions,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LocalizedText {
    #[serde(rename = "@lang", default)]
    lang: String,
    #[serde(rename = "$value", default)]
    text: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Parameters {
    #[serde(rename = "parameter", default)]
    parameters: Vec<Parameter>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Parameter {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@unique", default)]
    unique: String,
    #[serde(rename = "@required", default)]
    required: String,
    #[serde(default)]
    longdesc: LocalizedText,
    #[serde(default)]
    shortdesc: LocalizedText,
    #[serde(default)]
    content: Content,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Content {
    #[serde(rename = "@type", default)]
    type_: String,
    #[serde(rename = "@default", default)]
    default: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Actions {
    #[serde(rename = "action", default)]
    actions: Vec<Action>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Action {
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@timeout", default)]
    timeout: String,
    #[serde(rename = "@interval", default)]
    interval: String,
    #[serde(rename = "@depth", default)]
    depth: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Convert { root, output, save_xml } => {
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
            scan_and_process(&resource_d, &output_dir, save_xml)?;
        }
    }

    Ok(())
}

fn scan_and_process(resource_d: &Path, output_dir: &Path, save_xml: bool) -> Result<()> {
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
                        if let Err(e) = process_agent(&agent_path, &provider_output_dir, save_xml) {
                            eprintln!("Failed to process agent {}: {}", agent_path.display(), e);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_agent(agent_path: &Path, output_dir: &Path, save_xml: bool) -> Result<()> {
    let agent_name = agent_path.file_name().unwrap().to_string_lossy();
    println!("Processing Agent: {}", agent_name);

    // Run the agent with meta-data argument
    // Use timeout command to prevent hanging
    // On macOS/Linux, we can execute the file directly
    let output = Command::new(agent_path)
        .arg("meta-data")
        .env("OCF_ROOT", "/usr/lib/ocf") // Ensure env is set for the script itself if needed
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

    Ok(())
}

fn generate_ts(ra: &ResourceAgent, agent_name: &str) -> String {
    let safe_agent_name = agent_name.replace("-", "_").replace(".", "_");
    let mut ts = String::new();

    ts.push_str(&format!("export interface {} {{\n", safe_agent_name));
    ts.push_str("  name: string;\n");
    ts.push_str("  version: string;\n");
    ts.push_str("  shortdesc: string;\n");
    ts.push_str("  longdesc: string;\n");
    ts.push_str("  parameters: Parameter[];\n");
    ts.push_str("  actions: Action[];\n");
    ts.push_str("}\n\n");

    ts.push_str("export interface Parameter {\n");
    ts.push_str("  name: string;\n");
    ts.push_str("  unique: boolean;\n");
    ts.push_str("  required: boolean;\n");
    ts.push_str("  shortdesc: string;\n");
    ts.push_str("  longdesc: string;\n");
    ts.push_str("  type: string;\n");
    ts.push_str("  default: string;\n");
    ts.push_str("}\n\n");

    ts.push_str("export interface Action {\n");
    ts.push_str("  name: string;\n");
    ts.push_str("  timeout: string;\n");
    ts.push_str("  interval: string;\n");
    ts.push_str("  depth: string;\n");
    ts.push_str("}\n\n");

    ts.push_str(&format!("export const {}_DATA: {} = {{\n", safe_agent_name, safe_agent_name));
    ts.push_str(&format!("  name: \"{}\",\n", ra.name));
    ts.push_str(&format!("  version: \"{}\",\n", ra.version));
    ts.push_str(&format!("  shortdesc: {:?},\n", ra.shortdesc.text.trim()));
    ts.push_str(&format!("  longdesc: {:?},\n", ra.longdesc.text.trim()));

    ts.push_str("  parameters: [\n");
    for param in &ra.parameters.parameters {
        ts.push_str("    {\n");
        ts.push_str(&format!("      name: \"{}\",\n", param.name));
        ts.push_str(&format!("      unique: {},\n", param.unique == "1"));
        ts.push_str(&format!("      required: {},\n", param.required == "1"));
        ts.push_str(&format!("      shortdesc: {:?},\n", param.shortdesc.text.trim()));
        ts.push_str(&format!("      longdesc: {:?},\n", param.longdesc.text.trim()));
        ts.push_str(&format!("      type: \"{}\",\n", param.content.type_));
        ts.push_str(&format!("      default: \"{}\",\n", param.content.default));
        ts.push_str("    },\n");
    }
    ts.push_str("  ],
");

    ts.push_str("  actions: [\n");
    for action in &ra.actions.actions {
        ts.push_str("    {\n");
        ts.push_str(&format!("      name: \"{}\",\n", action.name));
        ts.push_str(&format!("      timeout: \"{}\",\n", action.timeout));
        ts.push_str(&format!("      interval: \"{}\",\n", action.interval));
        ts.push_str(&format!("      depth: \"{}\",\n", action.depth));
        ts.push_str("    },\n");
    }
    ts.push_str("  ]
");

    ts.push_str("};
");

    ts
}
