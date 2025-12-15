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

### 1. Build from Source

```bash
# Build UI and Backend
make build

# The binary will be at: target/release/drbd-ha
```

### 2. Setup SSH Trust

The manager node (where `drbd-ha` runs) needs passwordless SSH root access to all managed nodes (including itself if it's part of the cluster logic, though local operations are optimized to direct syscalls).

```bash
# Run the helper script as root
sudo ./scripts/setup-ssh.sh
# Enter the IPs of your other nodes when prompted
```

### 3. Deploy as System Service

We recommend running `drbd-ha` as a systemd service.

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
Description=DRBD HA Backend Service
After=network.target

[Service]
User=root
Group=root
WorkingDirectory=/opt/drbd-ha
ExecStart=/opt/drbd-ha/drbd-ha
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
```

**Step 4: Start Service**

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now drbd-ha
sudo systemctl status drbd-ha
```

## Usage

*   **Web UI**: Open `http://<server-ip>:3373` in your browser.
*   **API Docs**: Open `http://<server-ip>:3373/swagger-ui/` for interactive API documentation.
*   **Logs**: Check `/var/log/drbd-ha/drbd-ha.log` or `journalctl -u drbd-ha -f`.

## Architecture

*   **Language**: Rust (Axum framework)
*   **Frontend**: React + Ant Design (Embedded in binary)
*   **Database**: SQLite (Embedded)
*   **Execution Model**:
    *   **Local**: Direct system calls (LVM, DRBD, Systemd).
    *   **Remote**: SSH execution.

## License

Apache-2.0
