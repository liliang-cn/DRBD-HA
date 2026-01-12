#!/bin/bash
set -e

# DRBD-HA Release Package Preparation Script
# Creates a complete release package for internal testing/deployment

VERSION=${1:-"dev-$(date +%Y%m%d)"}
RELEASE_DIR="drbd-ha-release-${VERSION}"

echo "=== Preparing DRBD-HA Release Package ==="
echo "Version: ${VERSION}"
echo ""

# 1. Clean up old release if exists
if [[ -d "$RELEASE_DIR" ]]; then
    echo "Removing existing release directory..."
    rm -rf "$RELEASE_DIR"
fi

# 2. Create directory structure
echo "[1/7] Creating directory structure..."
mkdir -p "${RELEASE_DIR}"/{bin,config,systemd,scripts,docs}

# 3. Build binaries
echo "[2/7] Building release binaries..."
make release

# 4. Copy binary
echo "[3/7] Copying binary..."
cp target/release/drbd-ha "${RELEASE_DIR}/bin/"
chmod +x "${RELEASE_DIR}/bin/drbd-ha"

# 5. Copy configuration files
echo "[4/7] Copying configuration files..."
cp drbd-ha/config/default.toml "${RELEASE_DIR}/config/config.toml.example"

# 6. Copy systemd service file
echo "[5/7] Creating systemd service file..."
cat > "${RELEASE_DIR}/systemd/drbd-ha.service" << 'EOF'
[Unit]
Description=HA-Forge (drbd-ha) Orchestrator
After=network.target sshd.service

[Service]
Type=simple
ExecStart=/opt/drbd-ha/drbd-ha
WorkingDirectory=/opt/drbd-ha
Restart=always
RestartSec=5
User=root
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF

# 7. Create installation script
echo "[6/7] Creating installation script..."
cat > "${RELEASE_DIR}/scripts/install.sh" << 'EOF'
#!/bin/bash
set -e

echo "=== Installing DRBD-HA ==="

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo "Error: This script must be run as root" 
   exit 1
fi

INSTALL_DIR="/opt/drbd-ha"
CONFIG_DIR="/etc/drbd-ha"
LOG_DIR="/var/log/drbd-ha"

# 1. Create directories
echo "[1/5] Creating directories..."
mkdir -p "$INSTALL_DIR"
mkdir -p "$CONFIG_DIR"
mkdir -p "$LOG_DIR"

# 2. Copy binary
echo "[2/5] Installing binary..."
cp bin/drbd-ha "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/drbd-ha"

# 3. Copy configuration
echo "[3/5] Installing configuration..."
if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
    cp config/config.toml.example "$CONFIG_DIR/config.toml"
    echo "Configuration file created at $CONFIG_DIR/config.toml"
    echo "Please edit it to match your environment!"
else
    echo "Configuration file already exists at $CONFIG_DIR/config.toml (not overwriting)"
fi

# 4. Install systemd service
echo "[4/5] Installing systemd service..."
cp systemd/drbd-ha.service /etc/systemd/system/
systemctl daemon-reload

# 5. Done
echo "[5/5] Installation complete!"
echo ""
echo "Next steps:"
echo "  1. Edit configuration: $CONFIG_DIR/config.toml"
echo "  2. Enable service: systemctl enable drbd-ha"
echo "  3. Start service: systemctl start drbd-ha"
echo "  4. Check status: systemctl status drbd-ha"
echo "  5. Access Web UI: http://$(hostname -I | awk '{print $1}'):3373"
EOF

chmod +x "${RELEASE_DIR}/scripts/install.sh"

# 8. Create uninstall script
cat > "${RELEASE_DIR}/scripts/uninstall.sh" << 'EOF'
#!/bin/bash
set -e

echo "=== Uninstalling DRBD-HA ==="

# Check if running as root
if [[ $EUID -ne 0 ]]; then
   echo "Error: This script must be run as root" 
   exit 1
fi

# Stop and disable service
if systemctl is-active --quiet drbd-ha; then
    echo "Stopping drbd-ha service..."
    systemctl stop drbd-ha
fi

if systemctl is-enabled --quiet drbd-ha 2>/dev/null; then
    echo "Disabling drbd-ha service..."
    systemctl disable drbd-ha
fi

# Remove systemd service
if [[ -f /etc/systemd/system/drbd-ha.service ]]; then
    echo "Removing systemd service..."
    rm /etc/systemd/system/drbd-ha.service
    systemctl daemon-reload
fi

# Remove binary
if [[ -d /opt/drbd-ha ]]; then
    echo "Removing binary..."
    rm -rf /opt/drbd-ha
fi

# Ask about config and logs
read -p "Remove configuration files in /etc/drbd-ha? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf /etc/drbd-ha
    echo "Configuration removed"
fi

read -p "Remove log files in /var/log/drbd-ha? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    rm -rf /var/log/drbd-ha
    echo "Logs removed"
fi

echo "Uninstallation complete!"
EOF

chmod +x "${RELEASE_DIR}/scripts/uninstall.sh"

# 9. Copy documentation
echo "[7/7] Copying documentation..."
cp docs/DEPLOYMENT.md "${RELEASE_DIR}/docs/"
cp README.md "${RELEASE_DIR}/docs/"

# Create a quick start guide
cat > "${RELEASE_DIR}/docs/QUICKSTART.md" << 'EOF'
# DRBD-HA Quick Start Guide

## Prerequisites

Before installing, ensure you have:

1. **Linux System** (Ubuntu 20.04+, RHEL 8+, or similar)
2. **Root Access** on all nodes
3. **SSH Access** configured between nodes
4. **Dependencies installed** on all nodes:
   ```bash
   # Ubuntu/Debian
   apt-get install -y lvm2 drbd-utils drbd-dkms drbd-reactor
   
   # RHEL/CentOS
   yum install -y lvm2 drbd-utils kmod-drbd drbd-reactor
   ```

## Installation Steps

### 1. Extract Package
```bash
cd drbd-ha-release-*
```

### 2. Run Installation Script
```bash
sudo ./scripts/install.sh
```

### 3. Configure
Edit `/etc/drbd-ha/config.toml`:

```toml
[server]
host = "0.0.0.0"
port = 3373

[ssh]
default_user = "root"  # Change to your SSH user
connection_timeout_secs = 30

[log]
level = "info"
file = "/var/log/drbd-ha/drbd-ha.log"
```

### 4. Setup SSH Keys (if not already done)

On the controller node:
```bash
# Generate key
ssh-keygen -t ed25519 -f ~/.ssh/id_drbd_ha -N ""

# Copy to all managed nodes
ssh-copy-id -i ~/.ssh/id_drbd_ha.pub root@node1
ssh-copy-id -i ~/.ssh/id_drbd_ha.pub root@node2
ssh-copy-id -i ~/.ssh/id_drbd_ha.pub root@node3

# Verify
ssh -i ~/.ssh/id_drbd_ha root@node1 "sudo id"
```

### 5. Start Service
```bash
sudo systemctl enable drbd-ha
sudo systemctl start drbd-ha
sudo systemctl status drbd-ha
```

### 6. Access Web UI
Open browser: `http://<your-server-ip>:3373`

## First Steps in Web UI

1. **Add Nodes**: Click "Nodes" → "Add Node"
   - Enter node IP, SSH port, username
   - Provide SSH private key path (e.g., `/root/.ssh/id_drbd_ha`)

2. **Create Storage**: Go to "Storage" → "Create LVM/ZFS"
   - Select nodes and configure storage

3. **Create DRBD Resource**: "Resources" → "Create Resource"
   - Configure replication between nodes

4. **Setup HA Profile**: "HA Profiles" → "Create Profile"
   - Choose Simple mode for systemd services
   - Choose Advanced mode for OCF agents

## Troubleshooting

### Service won't start
```bash
# Check logs
journalctl -u drbd-ha -f

# Check log file
tail -f /var/log/drbd-ha/drbd-ha.log
```

### SSH connection issues
```bash
# Test SSH manually
ssh -i /root/.ssh/id_drbd_ha root@node1 "hostname"

# Check SSH key permissions
ls -la ~/.ssh/id_drbd_ha
chmod 600 ~/.ssh/id_drbd_ha
```

### Port already in use
```bash
# Check what's using port 3373
sudo lsof -i :3373

# Change port in /etc/drbd-ha/config.toml
```

## Getting Help

- **API Documentation**: http://your-server:3373/swagger-ui/
- **Full Documentation**: See `docs/DEPLOYMENT.md`
- **Logs**: `/var/log/drbd-ha/drbd-ha.log`
EOF

# 10. Create README for the release package
cat > "${RELEASE_DIR}/README.txt" << EOF
===========================================
  DRBD-HA Release Package ${VERSION}
===========================================

This package contains everything needed to deploy DRBD-HA.

CONTENTS:
  bin/           - Compiled binary (drbd-ha)
  config/        - Configuration example file
  systemd/       - Systemd service unit file
  scripts/       - Installation and management scripts
  docs/          - Documentation

QUICK INSTALLATION:
  1. sudo ./scripts/install.sh
  2. Edit /etc/drbd-ha/config.toml
  3. sudo systemctl start drbd-ha
  4. Access http://your-server:3373

DOCUMENTATION:
  - docs/QUICKSTART.md   - Quick start guide
  - docs/DEPLOYMENT.md   - Full deployment guide
  - docs/README.md       - Project overview

SUPPORT:
  For issues or questions, check the logs:
    - System logs: journalctl -u drbd-ha -f
    - Application logs: /var/log/drbd-ha/drbd-ha.log

VERSION: ${VERSION}
BUILD DATE: $(date)
EOF

# 11. Package everything
echo ""
echo "=== Creating archive ==="
tar czf "${RELEASE_DIR}.tar.gz" "${RELEASE_DIR}"

# 12. Summary
echo ""
echo "=== Release Package Ready ==="
echo ""
echo "Package: ${RELEASE_DIR}.tar.gz"
echo "Size: $(du -h ${RELEASE_DIR}.tar.gz | cut -f1)"
echo ""
echo "Contents:"
ls -lh "${RELEASE_DIR}"/
echo ""
echo "To deploy to testers:"
echo "  1. Send: ${RELEASE_DIR}.tar.gz"
echo "  2. Extract: tar xzf ${RELEASE_DIR}.tar.gz"
echo "  3. Install: cd ${RELEASE_DIR} && sudo ./scripts/install.sh"
echo ""
echo "The package includes:"
echo "  ✓ Binary (drbd-ha)"
echo "  ✓ Configuration example"
echo "  ✓ Systemd service file"
echo "  ✓ Installation script"
echo "  ✓ Uninstall script"
echo "  ✓ Quick start guide"
echo "  ✓ Full documentation"
