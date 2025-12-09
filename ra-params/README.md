# ra-params

**ra-params** is a CLI tool designed to automatically discover OCF Resource Agents, execute their `meta-data` command, and convert the resulting XML metadata into structured JSON and TypeScript definitions. This is particularly useful for generating frontend-consumable data structures for high-availability cluster management UIs.

## Features

*   **Automatic Discovery**: Scans `OCF_ROOT` (default: `/usr/lib/ocf`) to find available resource agents.
*   **Live Extraction**: Executes the agent itself (`./AgentName meta-data`) to get the most accurate and up-to-date metadata.
*   **Structured Output**: Generates both JSON and TypeScript interfaces.
*   **Provider Aware**: Preserves the directory structure of OCF providers (e.g., `heartbeat`, `linbit`, `pacemaker`).
*   **Safe Execution**: Checks for executable permissions before running and includes timeouts to prevent hanging.
*   **XML Archiving**: Optionally saves the raw XML output for debugging or reference.

## Installation

This tool is part of the `drbd-ha` workspace. To build it from source:

```bash
# Build the entire workspace (includes ra-params)
cargo build --release --workspace

# The binary will be available at:
# ./target/release/ra-params
```

## Usage

### Basic Usage

By default, `ra-params` looks for agents in `/usr/lib/ocf` and outputs files to the current directory.

```bash
./ra-params convert
```

### Specify Output Directory

Generate files into a specific folder:

```bash
./ra-params convert --output ./generated-agents
```

### Specify OCF Root

If your OCF agents are installed in a non-standard location (or for testing):

```bash
./ra-params convert --root /opt/cluster/lib/ocf
```

You can also use the `OCF_ROOT` environment variable:

```bash
export OCF_ROOT=/opt/cluster/lib/ocf
./ra-params convert
```

### Save Raw XML

To keep the original XML metadata files alongside the generated JSON and TS files:

```bash
./ra-params convert --save-xml
```

## Output Structure

The tool maintains the provider hierarchy found in the `resource.d` directory. For example:

```text
generated-agents/
├── heartbeat/
│   ├── IPaddr2.json
│   ├── IPaddr2.ts
│   ├── IPaddr2.xml (if --save-xml is used)
│   ├── nginx.json
│   └── nginx.ts
└── linbit/
    ├── drbd.json
    └── drbd.ts
```

## TypeScript Output Example

The generated TypeScript file contains interfaces and a constant with the metadata:

```typescript
export interface IPaddr2 {
  name: string;
  version: string;
  shortdesc: string;
  longdesc: string;
  parameters: Parameter[];
  actions: Action[];
}

// ... Parameter and Action interfaces ...

export const IPaddr2_DATA: IPaddr2 = {
  name: "IPaddr2",
  version: "1.0",
  shortdesc: "Manages virtual IPv4 and IPv6 addresses...",
  // ... full metadata ...
};
```
