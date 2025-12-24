# DRBD HA

A modern, Rust-based High Availability management system for DRBD, LVM, and Systemd services.

## Features

*   **Cluster Management**: Manage multiple nodes via SSH.
*   **Storage Management**: Create LVM Volume Groups and Logical Volumes across the cluster.
*   **DRBD Resources**: Create, initialize, and manage DRBD resources automatically.
*   **High Availability**: Define HA profiles (Generic, NFS, iSCSI, NVMe-oF) that automatically failover using `drbd-reactor`.
*   **Configuration Discovery**: Automatically discover and import existing `drbd-reactor` configurations from `/etc/drbd-reactor.d/`.
*   **Observability**: Real-time dashboard, dual-channel logging (console + file), and Swagger API documentation.

## Requirements

*   **Operating System**: Linux (tested on Ubuntu/Debian, RHEL/CentOS).
*   **Permissions**: **Must run as root** on the management node.
*   **Dependencies**:
    *   `lvm2`
    *   `drbd-utils` & `drbd-dkms` (Kernel module loaded)
    *   `drbd-reactor`
    *   `systemd`
    *   `ssh` (Client)

## Installation & Deployment

### Quick Deployment (Recommended)

The easiest way to deploy drbd-ha is using the automated deployment script:

```bash
# Deploy everything (build + install + start service)
sudo ./scripts/deploy.sh

# Or skip build if you already have binaries
sudo ./scripts/deploy.sh --skip-build

# For development mode (debug build)
sudo ./scripts/deploy.sh --dev
```

The deployment script will:
1. Check system dependencies (lvm2, drbd-utils, drbd-reactor, systemd)
2. Build UI and Backend (unless `--skip-build` is used)
3. Install binary to `/opt/drbd-ha/`
4. Create directories (`/etc/drbd-ha`, `/var/lib/drbd-ha`, `/var/log/drbd-ha`)
5. Install configuration file
6. Create and start systemd service

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

#### 2. Setup SSH Trust

The manager node (where `drbd-ha` runs) needs passwordless SSH root access to all managed nodes.

```bash
# Run the helper script as root
sudo ./scripts/setup-ssh.sh
# Enter the IPs of your other nodes when prompted
```

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
