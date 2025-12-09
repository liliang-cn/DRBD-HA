use crate::models::ResourceAgent;
use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn generate_ts(ra: &ResourceAgent, agent_name: &str) -> String {
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

    ts.push_str(&format!(
        "export const {}_DATA: {} = {{ \n",
        safe_agent_name,
        safe_agent_name,
    ));
    ts.push_str(&format!("  name: \"{}\" ,\n", ra.name));
    ts.push_str(&format!("  version: \"{}\" ,\n", ra.version));
    ts.push_str(&format!("  shortdesc: {:?} ,\n", ra.shortdesc.text.trim()));
    ts.push_str(&format!("  longdesc: {:?} ,\n", ra.longdesc.text.trim()));

    ts.push_str("  parameters: [\n");
    for param in &ra.parameters.parameters {
        ts.push_str("    {\n");
        ts.push_str(&format!("      name: \"{}\" ,\n", param.name));
        ts.push_str(&format!("      unique: {} ,\n", param.unique == "1"));
        ts.push_str(&format!("      required: {} ,\n", param.required == "1"));
        ts.push_str(&format!(
            "      shortdesc: {:?} ,\n",
            param.shortdesc.text.trim(),
        ));
        ts.push_str(&format!(
            "      longdesc: {:?} ,\n",
            param.longdesc.text.trim(),
        ));
        ts.push_str(&format!("      type: \"{}\" ,\n", param.content.type_));
        ts.push_str(&format!("      default: \"{}\" ,\n", param.content.default));
        ts.push_str("    },\n");
    }
    ts.push_str("  ],\n");

    ts.push_str("  actions: [\n");
    for action in &ra.actions.actions {
        ts.push_str("    {\n");
        ts.push_str(&format!("      name: \"{}\" ,\n", action.name));
        ts.push_str(&format!("      timeout: \"{}\" ,\n", action.timeout));
        ts.push_str(&format!("      interval: \"{}\" ,\n", action.interval));
        ts.push_str(&format!("      depth: \"{}\" ,\n", action.depth));
        ts.push_str("    },\n");
    }
    ts.push_str("  ]\n");

    ts.push_str("};\n");

    ts
}

pub fn generate_combined_files(agents: &[ResourceAgent], output_dir: &Path) -> Result<()> {
    // Generate combined JSON
    let json_path = output_dir.join("all_agents.json");
    let json_file = fs::File::create(&json_path)?;
    serde_json::to_writer_pretty(json_file, agents)?;
    println!("Generated Combined JSON: {}", json_path.display());

    // Generate combined TS
    let ts_path = output_dir.join("all_agents.ts");
    let mut ts = String::new();

    // Define interfaces once
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

    ts.push_str("export interface ResourceAgent {\n");
    ts.push_str("  name: string;\n");
    ts.push_str("  version: string;\n");
    ts.push_str("  shortdesc: string;\n");
    ts.push_str("  longdesc: string;\n");
    ts.push_str("  parameters: Parameter[];\n");
    ts.push_str("  actions: Action[];\n");
    ts.push_str("}\n\n");

    ts.push_str("export const ALL_AGENTS: ResourceAgent[] = [\n");

    for ra in agents {
        ts.push_str("  {\n");
        ts.push_str(&format!("    name: \"{}\" ,\n", ra.name));
        ts.push_str(&format!("    version: \"{}\" ,\n", ra.version));
        ts.push_str(&format!("    shortdesc: {:?} ,\n", ra.shortdesc.text.trim()));
        ts.push_str(&format!("    longdesc: {:?} ,\n", ra.longdesc.text.trim()));

        ts.push_str("    parameters: [\n");
        for param in &ra.parameters.parameters {
            ts.push_str("      {\n");
            ts.push_str(&format!("        name: \"{}\" ,\n", param.name));
            ts.push_str(&format!("        unique: {} ,\n", param.unique == "1"));
            ts.push_str(&format!("        required: {} ,\n", param.required == "1"));
            ts.push_str(&format!(
                "        shortdesc: {:?} ,\n",
                param.shortdesc.text.trim()
            ));
            ts.push_str(&format!(
                "        longdesc: {:?} ,\n",
                param.longdesc.text.trim()
            ));
            ts.push_str(&format!("        type: \"{}\" ,\n", param.content.type_));
            ts.push_str(&format!("        default: \"{}\" ,\n", param.content.default));
            ts.push_str("      },\n");
        }
        ts.push_str("    ],\n");

        ts.push_str("    actions: [\n");
        for action in &ra.actions.actions {
            ts.push_str("      {\n");
            ts.push_str(&format!("        name: \"{}\" ,\n", action.name));
            ts.push_str(&format!("        timeout: \"{}\" ,\n", action.timeout));
            ts.push_str(&format!("        interval: \"{}\" ,\n", action.interval));
            ts.push_str(&format!("        depth: \"{}\" ,\n", action.depth));
            ts.push_str("      },\n");
        }
        ts.push_str("    ]\n");
        ts.push_str("  },\n");
    }

    ts.push_str("];\n");

    let mut ts_file = fs::File::create(&ts_path)?;
    ts_file.write_all(ts.as_bytes())?;
    println!("Generated Combined TS: {}", ts_path.display());

    Ok(())
}
