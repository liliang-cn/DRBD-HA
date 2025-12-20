use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ra_params::{generate_combined_files, generate_ts, get_agent_metadata, list_agents, models::ResourceAgent};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

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
    /// Verify that a TypeScript file matches the source XML
    Verify {
        /// Path to the source XML file
        #[arg(long)]
        xml: PathBuf,

        /// Path to the generated TypeScript file
        #[arg(long)]
        ts: PathBuf,
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
            scan_and_process(&ocf_root, &output_dir, save_xml, combine)?;
        }
        Commands::Verify { xml, ts } => {
            verify_files(&xml, &ts)?;
        }
    }

    Ok(())
}

fn verify_files(xml_path: &Path, ts_path: &Path) -> Result<()> {
    println!("Verifying:");
    println!("  XML: {}", xml_path.display());
    println!("  TS:  {}", ts_path.display());

    // 1. Read and Parse XML
    let xml_content = fs::read_to_string(xml_path).context("Failed to read XML file")?;
    let ra: ResourceAgent = quick_xml::de::from_str(&xml_content).context("Failed to parse XML")?;

    // 2. Generate Expected TS
    let agent_name = &ra.name;
    let expected_ts = generate_ts(&ra, agent_name);

    // 3. Read Actual TS
    let actual_ts = fs::read_to_string(ts_path).context("Failed to read TS file")?;

    // 4. Compare
    if expected_ts.trim() == actual_ts.trim() {
        println!("✅ Verification SUCCESS: The TypeScript file matches the XML source.");
        Ok(())
    } else {
        eprintln!("❌ Verification FAILED: The files do not match.");

        let expected_path = PathBuf::from("expected.ts");
        fs::write(&expected_path, &expected_ts).context("Failed to write expected.ts")?;

        eprintln!("Written expected output to: {}", expected_path.display());
        eprintln!("You can check the difference with:");
        eprintln!("  diff {} {}", expected_path.display(), ts_path.display());

        anyhow::bail!("Verification failed");
    }
}

fn scan_and_process(
    ocf_root: &Path,
    output_dir: &Path,
    save_xml: bool,
    combine: bool,
) -> Result<()> {
    let mut all_agents = Vec::new();
    let mut success_count = 0;
    let mut fail_count = 0;
    let mut failures: Vec<(String, String)> = Vec::new();

    let agents = list_agents(ocf_root)?;
    
    for (provider, agent_name) in agents {
        let provider_output_dir = output_dir.join(&provider);
        fs::create_dir_all(&provider_output_dir)?;
        
        // Reconstruct path (a bit redundant but fits the loop)
        let agent_path = ocf_root.join("resource.d").join(&provider).join(&agent_name);

        match process_agent(&agent_path, &provider_output_dir, save_xml) {
            Ok(agent) => {
                success_count += 1;
                if combine {
                    all_agents.push(agent);
                }
            }
            Err(e) => {
                fail_count += 1;
                failures.push((agent_name.clone(), e.to_string()));
                eprintln!(
                    "Failed to process agent {}: {}",
                    agent_path.display(),
                    e
                );
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

fn process_agent(agent_path: &Path, output_dir: &Path, save_xml: bool) -> Result<ResourceAgent> {
    let agent_name = agent_path.file_name().unwrap().to_string_lossy();
    println!("Processing Agent: {}", agent_name);

    let (ra, xml_content) = get_agent_metadata(agent_path)?;

    // Optional: Save XML
    if save_xml {
        let xml_path = output_dir.join(format!("{}.xml", agent_name));
        let mut file = fs::File::create(&xml_path)?;
        file.write_all(xml_content.as_bytes())?;
        println!("  Saved XML: {}", xml_path.display());
    }

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
