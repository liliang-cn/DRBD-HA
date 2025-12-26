#!/bin/bash
set -e

#############################################################################
# DRBD HA Manager - Remote Installation Script
#
# This script is run on the remote server to install drbd-ha service.
# It is typically executed by the deploy.sh script.
#
# Usage: sudo ./install.sh
#
# This script expects to be run from /tmp/drbd-ha-deploy/ with:
#   - drbd-ha (binary)
#   - config.toml (configuration)
#############################################################################

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
INSTALL_DIR="/opt/drbd-ha"
CONFIG_DIR="/etc/drbd-ha"
DATA_DIR="/var/lib/drbd-ha"
LOG_DIR="/var/log/drbd-ha"
SERVICE_NAME="drbd-ha"
DEPLOY_DIR="/tmp/drbd-ha-deploy"

echo -e "${BLUE}=== DRBD HA Manager - Remote Installation ===${NC}"
echo ""

#############################################################################
# 1. Check if running as root
#############################################################################
check_root() {
    echo -e "${BLUE}[1/6] Checking privileges...${NC}"
    if [ "$EUID" -ne 0 ]; then
        echo -e "${RED}Error: This script must be run as root${NC}"
        echo "Please run: sudo $0"
        exit 1
    fi
    echo -e "${GREEN}✓ Running as root${NC}"
    echo ""
}

#############################################################################
# 2. Check system dependencies
#############################################################################
check_dependencies() {
    echo -e "${BLUE}[2/6] Checking system dependencies...${NC}"

    local missing_deps=()

    # Check required commands
    local commands=("lvm" "drbdsetup" "drbdadm" "systemctl" "ssh")
    for cmd in "${commands[@]}"; do
        if ! command -v "$cmd" &> /dev/null; then
            missing_deps+=("$cmd")
        fi
    done

    # Check drbd-reactor
    if ! systemctl list-units --all | grep -q drbd-reactor; then
        echo -e "${YELLOW}⚠ Warning: drbd-reactor service not found${NC}"
        echo "  drbd-reactor is required for HA failover to work"
        echo "  Install it from: https://github.com/LINBIT/drbd-reactor"
    fi

    # Check DRBD kernel module
    if ! lsmod | grep -q drbd; then
        echo -e "${YELLOW}⚠ Warning: DRBD kernel module not loaded${NC}"
        echo "  Load it with: modprobe drbd"
        echo "  Or install drbd-dkms package"
    fi

    if [ ${#missing_deps[@]} -gt 0 ]; then
        echo -e "${RED}Error: Missing required dependencies:${NC}"
        printf '  - %s\n' "${missing_deps[@]}"
        echo ""
        echo "Install them on Ubuntu/Debian:"
        echo "  sudo apt-get install lvm2 drbd-utils drbd-dkms"
        echo ""
        echo "Install them on RHEL/CentOS:"
        echo "  sudo yum install lvm2 drbd-utils drbd kmod-drbd"
        exit 1
    fi

    echo -e "${GREEN}✓ All dependencies satisfied${NC}"
    echo ""
}

#############################################################################
# 3. Create directories
#############################################################################
create_directories() {
    echo -e "${BLUE}[3/6] Creating directories...${NC}"

    mkdir -p "$INSTALL_DIR"
    mkdir -p "$CONFIG_DIR"
    mkdir -p "$DATA_DIR"
    mkdir -p "$LOG_DIR"

    # Set permissions
    chmod 700 "$CONFIG_DIR"
    chmod 755 "$INSTALL_DIR"
    chmod 755 "$DATA_DIR"
    chmod 755 "$LOG_DIR"

    echo -e "${GREEN}✓ Directories created:${NC}"
    echo "  Install:  $INSTALL_DIR"
    echo "  Config:   $CONFIG_DIR"
    echo "  Data:     $DATA_DIR"
    echo "  Logs:     $LOG_DIR"
    echo ""
}

#############################################################################
# 4. Install binary
#############################################################################
install_binary() {
    echo -e "${BLUE}[4/6] Installing binary...${NC}"

    if [ ! -f "$DEPLOY_DIR/drbd-ha" ]; then
        echo -e "${RED}Error: Binary not found at $DEPLOY_DIR/drbd-ha${NC}"
        echo "This script should be run from the deployment directory"
        exit 1
    fi

    # Stop service if running (to release the binary file)
    if systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
        echo "Stopping $SERVICE_NAME service..."
        systemctl stop "$SERVICE_NAME"
        sleep 1
    fi

    # Copy binary to /opt/drbd-ha
    cp "$DEPLOY_DIR/drbd-ha" "$INSTALL_DIR/$SERVICE_NAME"
    chmod +x "$INSTALL_DIR/$SERVICE_NAME"
    echo -e "${GREEN}✓ Binary installed: $INSTALL_DIR/$SERVICE_NAME${NC}"

    # Copy ra-params to /usr/local/bin (in PATH)
    if [ -f "$DEPLOY_DIR/ra-params" ]; then
        cp "$DEPLOY_DIR/ra-params" "/usr/local/bin/ra-params"
        chmod +x "/usr/local/bin/ra-params"
        echo -e "${GREEN}✓ ra-params installed: /usr/local/bin/ra-params${NC}"
    else
        echo -e "${YELLOW}⊙ ra-params not found, skipping${NC}"
    fi

    echo ""
}

#############################################################################
# 5. Install/Update configuration
#############################################################################
install_config() {
    echo -e "${BLUE}[5/6] Installing configuration...${NC}"

    local config_file="$CONFIG_DIR/config.toml"

    if [ -f "$DEPLOY_DIR/config.toml" ]; then
        # Backup existing config
        if [ -f "$config_file" ]; then
            cp "$config_file" "${config_file}.backup.$(date +%Y%m%d%H%M%S)"
            echo -e "${YELLOW}⊙ Backed up existing config${NC}"
        fi

        # Copy new config
        cp "$DEPLOY_DIR/config.toml" "$config_file"
        echo -e "${GREEN}✓ Configuration installed: $config_file${NC}"
    else
        # Create minimal config
        cat > "$config_file" << EOF
[server]
host = "0.0.0.0"
port = 3373

[log]
level = "info"
file = "$LOG_DIR/drbd-ha.log"
EOF
        echo -e "${YELLOW}⊙ Created minimal configuration${NC}"
    fi

    echo ""
}

#############################################################################
# 6. Create systemd service
#############################################################################
create_service() {
    echo -e "${BLUE}[6/6] Creating systemd service...${NC}"

    local service_file="/etc/systemd/system/${SERVICE_NAME}.service"

    cat > "$service_file" << EOF
[Unit]
Description=DRBD HA Manager Service
Documentation=https://github.com/LINBIT/drbd-ha
After=network.target drbd-reactor.service
Wants=drbd-reactor.service

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=$INSTALL_DIR
ExecStart=$INSTALL_DIR/$SERVICE_NAME --config $CONFIG_DIR/config.toml
Restart=always
RestartSec=3
StandardOutput=journal
StandardError=journal

# Security
NoNewPrivileges=true
PrivateTmp=true

# Resource limits
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF

    # Reload systemd and start service
    systemctl daemon-reload

    # Enable service
    systemctl enable "$SERVICE_NAME"

    # Check if service is already running
    if systemctl is-active --quiet "$SERVICE_NAME"; then
        echo "Service is already running, restarting..."
        systemctl restart "$SERVICE_NAME"
    else
        systemctl start "$SERVICE_NAME"
    fi

    # Check status
    sleep 2
    if systemctl is-active --quiet "$SERVICE_NAME"; then
        echo -e "${GREEN}✓ Service started successfully${NC}"
        echo ""
        echo "Service status:"
        systemctl status "$SERVICE_NAME" --no-pager -l
    else
        echo -e "${RED}✗ Failed to start service${NC}"
        echo ""
        echo "Check logs:"
        echo "  journalctl -u $SERVICE_NAME -n 50"
        echo "  cat $LOG_DIR/drbd-ha.log"
        exit 1
    fi
}

#############################################################################
# Show installation summary
#############################################################################
show_summary() {
    echo ""
    echo -e "${GREEN}=== Installation Complete ===${NC}"
    echo ""
    echo -e "${BLUE}Installation Locations:${NC}"
    echo "  Binary:   $INSTALL_DIR/$SERVICE_NAME"
    echo "  Tool:     /usr/local/bin/ra-params"
    echo "  Config:   $CONFIG_DIR/config.toml"
    echo "  Logs:     $LOG_DIR/drbd-ha.log"
    echo ""
    echo -e "${BLUE}Service Commands:${NC}"
    echo "  Start:    systemctl start $SERVICE_NAME"
    echo "  Stop:     systemctl stop $SERVICE_NAME"
    echo "  Restart:  systemctl restart $SERVICE_NAME"
    echo "  Status:   systemctl status $SERVICE_NAME"
    echo "  Logs:     journalctl -u $SERVICE_NAME -f"
    echo ""
    echo -e "${BLUE}Web Interface:${NC}"
    echo "  URL:      http://$(hostname -I | awk '{print $1}'):3373"
    echo "  API Docs: http://$(hostname -I | awk '{print $1}'):3373/swagger-ui/"
    echo ""
    echo -e "${YELLOW}Next Steps:${NC}"
    echo "  1. Configure SSH keys for root user to access cluster nodes:"
    echo "     sudo -i"
    echo "     ssh-keygen -t rsa -b 4096"
    echo "     ssh-copy-id <user>@<node>"
    echo "     ssh -o BatchMode=yes <user>@<node> echo ok  # Test connection"
    echo ""
    echo "     IMPORTANT: The service runs as root, so SSH keys must be"
    echo "     configured for the root user (/root/.ssh/), not your regular user."
    echo ""
    echo "  2. Add nodes via Web UI or API"
    echo "  3. Create DRBD resources and HA profiles"
    echo ""
}

#############################################################################
# Main execution
#############################################################################
main() {
    check_root
    check_dependencies
    create_directories
    install_binary
    install_config
    create_service
    show_summary
}

# Run main function
main "$@"
