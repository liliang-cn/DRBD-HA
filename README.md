# DRBD HA

A modern, Rust-based High Availability management system for DRBD, LVM, and Systemd services.

## Features

*   **Cluster Management**: Manage multiple nodes via SSH.
*   **Storage Management**: Create LVM Volume Groups and Logical Volumes (with thin pool support) and ZFS volumes across the cluster.
*   **DRBD Resources**: Create, initialize, and manage DRBD resources automatically.
*   **High Availability**: Define HA profiles for systemd services that automatically failover using `drbd-reactor`.
*   **Configuration Discovery**: Automatically discover and import existing `drbd-reactor` configurations from `/etc/drbd-reactor.d/`.
*   **Observability**: Real-time dashboard, dual-channel logging (console + file), and Swagger API documentation.

## Requirements

*   **Operating System**: Linux (tested on Ubuntu/Debian, RHEL/CentOS).
*   **Permissions**: **Must run as root** on the management node.
*   **Remote Access**: Managed nodes must allow passwordless SSH. If the remote SSH user is not `root`, it must also allow passwordless sudo (`sudo -n`).
*   **Dependencies**:
    *   `lvm2`
    *   `drbd-utils` & `drbd-dkms` (Kernel module loaded)
    *   `drbd-reactor`
    *   `systemd`
    *   `ssh` (Client)

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

**The drbd-ha service runs as root on the controller**, so SSH keys must be configured for the **root** user on the machine running `drbd-ha`.

On managed nodes, you have two supported options:

- SSH directly as `root`
- SSH as a non-root user that has passwordless sudo (`sudo -n`)

On the machine where drbd-ha will run:

```bash
# Switch to root shell
sudo -i

# Generate SSH key (if not exists)
ssh-keygen -t rsa -b 4096

# Option A: SSH as root on each cluster node
ssh-copy-id root@orange2
ssh-copy-id root@orange3

# Test root SSH access
ssh -o BatchMode=yes root@orange2 echo ok

# Option B: SSH as a non-root user with passwordless sudo
# First, ensure the user can sudo without a password on the managed nodes:
#   sudo visudo
#   <user> ALL=(ALL) NOPASSWD:ALL
ssh-copy-id ubuntu@orange2
ssh-copy-id ubuntu@orange3

# Test SSH and passwordless sudo
ssh -o BatchMode=yes ubuntu@orange2 echo ok
ssh -o BatchMode=yes ubuntu@orange2 "sudo -n true"
```

**Why root on the controller?** The drbd-ha service manages DRBD, LVM, and systemd services which require root privileges. It runs as root and uses SSH to execute commands on remote nodes, so SSH keys must be in `/root/.ssh/`, not your regular user's home directory.

**How node checks work:** The built-in node health check only reports a remote node as online when passwordless SSH works and, for non-root SSH users, `sudo -n` also succeeds.

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

## Architecture

*   **Language**: Rust (Axum framework)
*   **Frontend**: React + Ant Design (Embedded in binary)
*   **Configuration Storage**: TOML files (nodes.toml) and DRBD configuration files.
*   **Execution Model**:
    *   **Local**: Direct system calls (LVM, DRBD, Systemd).
    *   **Remote**: SSH execution.

## License

Apache-2.0
