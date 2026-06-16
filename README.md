# DRBD HA

A modern, Rust-based High Availability management system for DRBD, LVM, and Systemd services.

## Features

*   **Cluster Management**: Manage multiple nodes via SSH from either an embedded controller node or an external controller host.
*   **Storage Management**: Create LVM Volume Groups and Logical Volumes (with thin pool support) and ZFS volumes across the cluster.
*   **DRBD Resources**: Create, initialize, and manage DRBD resources automatically.
*   **High Availability**: Define HA profiles for systemd services that automatically failover using `drbd-reactor`.
*   **Configuration Discovery**: Automatically discover and import existing `drbd-reactor` configurations from `/etc/drbd-reactor.d/`.
*   **AI Agent Integration (MCP)**: A built-in [Model Context Protocol](https://modelcontextprotocol.io) server at `/mcp` exposes cluster operations as tools, with bundled operational playbooks as prompts. See [AI Agent Integration](#ai-agent-integration-mcp).
*   **Observability**: Real-time dashboard, dual-channel logging (console + file), and Swagger API documentation.

## Requirements

*   **Operating System**:
    *   `embedded` mode: Linux only (tested on Ubuntu/Debian, RHEL/CentOS)
    *   `external` mode: Linux, macOS, or Windows, as long as the host can reach managed nodes over SSH
*   **Permissions**:
    *   `embedded` mode: `drbd-ha` must run as `root` on the controller host
    *   `external` mode: the management host does not need local DRBD/LVM/systemd privileges; only SSH client access to managed nodes is required
*   **Remote Access**: Managed nodes must allow passwordless SSH for the SSH account you actually use. If that user is not `root`, it must also allow passwordless sudo (`sudo -n`).
*   **Dependencies**:
    *   Managed nodes: `lvm2`, `drbd-utils` & `drbd-dkms`, `drbd-reactor`, `systemd`
    *   Controller host: `ssh` client
*   **Default SSH User**: If you do not set one explicitly, `drbd-ha` uses `DRBD_HA_SSH_USER`, then the current login user, and finally falls back to `root`.

## Controller Modes

`drbd-ha` supports two deployment modes:

- `external`: the default mode. `drbd-ha` runs on a separate management host outside the cluster and talks to managed nodes purely over SSH.
- `embedded`: opt-in mode. `drbd-ha` runs on one of the managed cluster nodes and treats that node as the controller execution target.

Example external controller configuration:

```toml
[controller]
mode = "external"
# optional advanced override; usually you do not need this
# proxy_host = "gui01"
# optional; defaults to [ssh].default_port / [ssh].default_user
# proxy_port = 22
# proxy_user = "cluster-admin"

[ssh]
# optional global default for all nodes
# default_user = "cluster-admin"
```

In `external` mode:

- The machine running `drbd-ha` does not need DRBD/LVM/systemd cluster state locally.
- The machine running `drbd-ha` can be a regular Linux/macOS/Windows host as long as it can reach the managed nodes over SSH.
- You only need to configure managed nodes in the UI/API and make SSH work. `drbd-ha` will automatically choose a managed node when it needs cluster-side config or system operations.
- `proxy_host` remains available only as an advanced override if you want to pin those controller-side operations to a specific node.
- Local controller config/state defaults to platform-appropriate user paths when `/etc/drbd-ha` is not present.
- Startup automatically detects the controller platform. On non-Linux hosts, `drbd-ha` automatically forces `mode = "external"` for SSH-only management.
- `/api/v1/health` reports both the detected `platform` and the active `controller_mode`.

## Installation & Deployment

### Quick Deployment (Recommended)

The easiest way to deploy drbd-ha is using the automated deployment script that builds locally and deploys to remote servers:

```bash
# Deploy to remote server (builds locally, installs remotely)
./scripts/deploy.sh root@orange1

# Deploy pre-built binaries (skip build)
./scripts/deploy.sh root@orange1 --skip-build

# Debug build
./scripts/deploy.sh root@orange1 --dev

# Deploy to multiple servers
for host in orange1 orange2 orange3; do
    ./scripts/deploy.sh root@$host
done
```

The deployment script will:
1. **Build locally** (unless `--skip-build` is used)
   - Compiles Rust backend
   - Builds React UI
   - Embeds UI into binary
2. **Transfer files via SCP** to remote server
   - Binary → `/tmp/drbd-ha-deploy/drbd-ha`
   - Config → `/tmp/drbd-ha-deploy/config.toml`
   - Install script → `/tmp/drbd-ha-deploy/install.sh`
3. **Execute install script** on remote server via SSH
   - Creates directories (`/opt/drbd-ha`, `/etc/drbd-ha`, `/var/lib/drbd-ha`, `/var/log/drbd-ha`)
   - Installs binary to `/opt/drbd-ha/drbd-ha`
   - Creates systemd service
   - Starts the service

**Requirements:**
- **Local Machine**: Makefile, ssh/scp commands, SSH access to remote server
- **Remote Server**: Root/sudo privileges, Linux with systemd, lvm2, drbd-utils, drbd-reactor

### Updating Existing Deployment

To update an already-deployed server without full reinstallation:

```bash
# Build and update
./scripts/update.sh root@orange1

# Update with existing binary (skip build)
./scripts/update.sh root@orange1 --skip-build

# Debug build
./scripts/update.sh root@orange1 --dev
```

The update script will:
1. **Build locally** (unless `--skip-build` is used)
2. **Stop the service** on remote server
3. **Upload new binary** via SCP
4. **Replace the old binary**
5. **Start the service**

This preserves all configuration, data, and logs while updating the binary.

### Manual Installation

If you prefer manual installation, follow these steps:

#### 1. Build from Source

```bash
# Build UI and Backend
make build

# Or build release binaries
make release

# The binary will be at: target/release/drbd-ha
```

#### 2. Setup SSH Access (IMPORTANT)

Use SSH keys for the account configured on each managed node. You can set a global `[ssh].default_user`, or leave the field empty per node and let `drbd-ha` use that default.

Supported remote access modes:

- SSH directly as `root`
- SSH as a non-root user that has passwordless sudo (`sudo -n`)

On the machine where drbd-ha will run:

```bash
# Generate SSH key (if not exists)
ssh-keygen -t rsa -b 4096

# Option A: root SSH on each cluster node
ssh-copy-id root@orange2
ssh-copy-id root@orange3

# Test root SSH access
ssh -o BatchMode=yes root@orange2 echo ok

# Option B: non-root user with passwordless sudo
# First, ensure the user can sudo without a password on the managed nodes:
#   sudo visudo
#   <user> ALL=(ALL) NOPASSWD:ALL
ssh-copy-id ubuntu@orange2
ssh-copy-id ubuntu@orange3

# Test SSH and passwordless sudo
ssh -o BatchMode=yes ubuntu@orange2 echo ok
ssh -o BatchMode=yes ubuntu@orange2 "sudo -n true"
```

**How node checks work:** The built-in node health check only reports a remote node as online when passwordless SSH works and, for non-root SSH users, `sudo -n` also succeeds.

**External mode note:** when `[controller].mode = "external"`, the management host only needs SSH connectivity to the cluster nodes. If you optionally pin `proxy_host`, that node must also satisfy the same passwordless SSH and sudo requirements.

#### 3. Deploy as System Service

**Step 1: Install Binary & Config**

```bash
sudo mkdir -p /opt/drbd-ha /etc/drbd-ha /var/lib/drbd-ha /var/log/drbd-ha
sudo cp target/release/drbd-ha /opt/drbd-ha/
sudo cp config/default.toml /etc/drbd-ha/config.toml
```

**Step 2: Configure Logging (Optional)**

Edit `/etc/drbd-ha/config.toml` to enable file logging:

```toml
[log]
level = "info"
file = "/var/log/drbd-ha/drbd-ha.log"
```

If you want the controller host itself to behave as a managed cluster node, explicitly enable embedded mode:

```toml
[controller]
mode = "embedded"
```

**Step 3: Create Systemd Service**

Create `/etc/systemd/system/drbd-ha.service`:

```ini
[Unit]
Description=DRBD HA Manager Service
Documentation=https://github.com/LINBIT/drbd-ha
After=network.target drbd-reactor.service
Wants=drbd-reactor.service

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=/opt/drbd-ha
ExecStart=/opt/drbd-ha/drbd-ha --config /etc/drbd-ha/config.toml
Restart=always
RestartSec=3
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

**Step 4: Start Service**

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now drbd-ha
sudo systemctl status drbd-ha
```

### Uninstallation

To remove the service:

```bash
# Remove service but keep configuration/data
sudo ./scripts/uninstall.sh

# Remove everything including configuration/data
sudo ./scripts/uninstall.sh --purge-all
```

## Usage

*   **Web UI**: Open `http://<server-ip>:3373` in your browser.
*   **API Docs**: Open `http://<server-ip>:3373/swagger-ui/` for interactive API documentation.
*   **Logs**: Check `/var/log/drbd-ha/drbd-ha.log` or `journalctl -u drbd-ha -f`.

## AI Agent Integration (MCP)

The backend embeds a [Model Context Protocol](https://modelcontextprotocol.io) server
(streamable HTTP) at `/mcp`, so an AI agent can operate the cluster through the same
operations the REST API exposes.

*   **Tools** — node inventory & disks, storage pools, DRBD resources (status / actions /
    logs), HA profile lifecycle (create / activate / deactivate / enable / evict / delete /
    TOML), `drbd-reactor` status / reload / logs, OCF agents, and systemd services.
*   **Prompts** — the operational playbooks under `skills/drbd-ha-ops/` (operating guide +
    safety rules, create-HA-service, failover & recovery, troubleshooting) are served to any
    connected agent so it follows the same conventions a human operator would.
*   **Read vs. mutate** — read-only tools (`list_*`, `get_*`, `*_status`, `*_logs`,
    `health`, `dashboard_summary`) are always safe; mutating tools change cluster state and
    should be verified with the status tools afterwards.

Connect with Claude Code (or any MCP client):

```bash
claude mcp add --transport http drbd-ha http://<server-ip>:3373/mcp
```

The repo also ships the same playbooks as a Claude Code skill in
`.claude/skills/drbd-ha-ops/`, and an `.mcp.json` pointing at a node's `/mcp` endpoint.

## Architecture

*   **Language**: Rust (Axum framework)
*   **Frontend**: React + shadcn/ui + Radix + Tailwind (embedded in the binary via `rust_embed`)
*   **Configuration Storage**: TOML files (nodes.toml) and DRBD configuration files.
*   **Execution Model**:
    *   **Local**: Direct system calls (LVM, DRBD, Systemd).
    *   **Remote**: SSH execution. All remote commands go through the
        [`dispatch-rs`](https://crates.io/crates/dispatch-rs) crate (a thin wrapper over the
        system `ssh`), giving every executor one consistent connection policy
        (see the `dispatch-config` workspace crate). Non-root nodes are driven via
        passwordless `sudo`.
*   **AI Integration**: An MCP server at `/mcp` (see [AI Agent Integration](#ai-agent-integration-mcp)).

## License

Apache-2.0
