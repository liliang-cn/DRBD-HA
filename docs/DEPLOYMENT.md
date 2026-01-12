# HA-Forge Deployment & Configuration Guide

This guide covers the end-to-end process of preparing the environment, installing the **HA-Forge (`drbd-ha`)** binary, configuring the service, and setting up the necessary SSH access for orchestration.

---

## Part 1: SSH & System Preparation

HA-Forge operates as an agentless orchestrator. It requires **passwordless SSH access** with **sudo privileges** from the Controller to all Managed Nodes.

### 1.1 Generate SSH Keys (On Controller)

On the machine where `drbd-ha` will run, generate an SSH key pair (ed25519 recommended).

```bash
# -N "" ensures no passphrase (required for automation)
ssh-keygen -t ed25519 -C "drbd-ha-controller" -f ~/.ssh/id_drbd_ha -N ""
```

### 1.2 Prepare User & Sudo (On All Nodes)

You can use `root` or a standard user (e.g., `ubuntu`). We recommend a standard user with `NOPASSWD` sudo.

1.  **Create User** (if needed):
    ```bash
    sudo useradd -m -s /bin/bash ubuntu
    ```
2.  **Configure Sudoers**:
    Run `sudo visudo` and add this line to the end:
    ```text
    ubuntu ALL=(ALL) NOPASSWD:ALL
    ```

### 1.3 Distribute Keys

Copy the public key from the Controller to all Managed Nodes.

```bash
# Replace with your actual node IPs
ssh-copy-id -i ~/.ssh/id_drbd_ha.pub ubuntu@192.168.1.101
ssh-copy-id -i ~/.ssh/id_drbd_ha.pub ubuntu@192.168.1.102
```

### 1.4 Verify Access

**Critical:** Verify you can log in and run sudo without **any** password prompts.

```bash
ssh -i ~/.ssh/id_drbd_ha ubuntu@192.168.1.101 "sudo id"
# Output should be: uid=0(root) gid=0(root) ...
```

---

## Part 2: Installation

Assumes you have the compiled binary `drbd-ha` (e.g., from `target/release/` or a release download).

### 2.1 Install Binary

We use `/opt/drbd-ha` as the standard installation directory.

```bash
# Create directory
sudo mkdir -p /opt/drbd-ha

# Copy binary (adjust source path as needed)
sudo cp ./target/release/drbd-ha /opt/drbd-ha/
sudo chmod +x /opt/drbd-ha/drbd-ha
```

### 2.2 Create Directories

Create standard directories for configuration and logs.

```bash
sudo mkdir -p /etc/drbd-ha
sudo mkdir -p /var/log/drbd-ha
```

---

## Part 3: Configuration

### 3.1 Config File

The application searches for configuration in this order:

1. Environment variable `CONFIG_PATH` (if set)
2. `config/default.toml` (relative to working directory)
3. `/etc/drbd-ha/config.toml`
4. Built-in defaults (if no config file found)

**For production deployment**, copy the default config:

```bash
sudo cp ./drbd-ha/config/default.toml /etc/drbd-ha/config.toml
```

Or use the environment variable in the systemd service (see Part 4).

### 3.2 Edit Configuration

Edit `/etc/drbd-ha/config.toml` to match your environment.

```toml
[server]
host = "0.0.0.0"
port = 3373

[ssh]
# The user configured in Step 1.2
default_user = "ubuntu"
connection_timeout_secs = 30
command_timeout_secs = 60
max_connections_per_host = 5
default_port = 22

[drbd]
config_path = "/etc/drbd.d"
reactor_config_path = "/etc/drbd-reactor.d"
systemd_unit_path = "/etc/systemd/system"

[auth]
# Authentication is disabled by default
enabled = false
# Uncomment and set if you enable auth
# token = "your-secret-token-here"

[log]
level = "info"
file = "/var/log/drbd-ha/drbd-ha.log"
```

**Important Notes:**

- `default_user` must match the user configured in Part 1.2
- This user needs passwordless SSH access and sudo privileges on all nodes
- Log file directory must exist (created in Step 2.2)

---

## Part 4: Systemd Service Setup

Run HA-Forge as a background service using systemd.

### 4.1 Create Unit File

Create `/etc/systemd/system/drbd-ha.service`:

```ini
[Unit]
Description=HA-Forge (drbd-ha) Orchestrator
After=network.target sshd.service

[Service]
Type=simple
# Path to your binary
ExecStart=/opt/drbd-ha/drbd-ha
# Working directory (useful for relative paths if any)
WorkingDirectory=/opt/drbd-ha
# Restart automatically on failure
Restart=always
RestartSec=5
# Run as root to allow local LVM/DRBD management if needed
User=root
# Environment variables
Environment=RUST_LOG=info
# Optional: Specify config file location explicitly
# Environment=CONFIG_PATH=/etc/drbd-ha/config.toml

[Install]
WantedBy=multi-user.target
```

### 4.2 Enable and Start

Reload systemd and start the service.

```bash
sudo systemctl daemon-reload
sudo systemctl enable drbd-ha
sudo systemctl start drbd-ha
```

### 4.3 Check Status

```bash
sudo systemctl status drbd-ha
# Check logs
tail -f /var/log/drbd-ha/drbd-ha.log
```

---

## Part 5: Access & Validation

Once the service is running:

1.  **Web UI**: Open `http://<controller-ip>:3373` in your browser.
2.  **API Docs**: Open `http://<controller-ip>:3373/swagger-ui/`.
3.  **Add Nodes**: Use the Web UI to add your nodes. You'll need:
    - **SSH Private Key**: The path to the private key generated in Part 1.1
      - If running as root: `/root/.ssh/id_drbd_ha`
      - If running as another user: `/home/<user>/.ssh/id_drbd_ha`
    - **Node Details**: IP address, SSH port (default 22), and username (matching `default_user` in config)

**Configuration Loading Order:**
The service searches for config in this order:

1. `$CONFIG_PATH` environment variable (if set in systemd unit)
2. `config/default.toml` (relative to WorkingDirectory)
3. `/etc/drbd-ha/config.toml`
4. Built-in defaults

For production, ensure `/etc/drbd-ha/config.toml` exists with proper permissions:

```bash
sudo chown root:root /etc/drbd-ha/config.toml
sudo chmod 600 /etc/drbd-ha/config.toml
```
