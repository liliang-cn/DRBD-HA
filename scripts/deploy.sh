#!/bin/bash
set -e

#############################################################################
# DRBD HA Manager - Deployment Script
#
# This script automates the deployment of drbd-ha service:
# - Checks system dependencies
# - Builds UI and Backend
# - Installs binary and configuration
# - Sets up systemd service
# - Starts the service
#
# Usage: sudo ./scripts/deploy.sh [--skip-build] [--dev]
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
BINARY_SOURCE="./target/release/drbd-ha"
CONFIG_SOURCE="./drbd-ha/config/default.toml"

# Parse arguments
SKIP_BUILD=false
BUILD_MODE="release"
for arg in "$@"; do
    case $arg in
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --dev)
            BUILD_MODE="debug"
            BINARY_SOURCE="./target/debug/drbd-ha"
            shift
            ;;
        *)
            ;;
    esac
done

echo -e "${BLUE}=== DRBD HA Manager - Deployment Script ===${NC}"
echo ""

#############################################################################
# 1. Check if running as root
#############################################################################
check_root() {
    echo -e "${BLUE}[1/8] Checking privileges...${NC}"
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
    echo -e "${BLUE}[2/8] Checking system dependencies...${NC}"

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
# 3. Build project
#############################################################################
build_project() {
    if [ "$SKIP_BUILD" = true ]; then
        echo -e "${BLUE}[3/8] Building project...${NC}"
        echo -e "${YELLOW}⊙ Skipping build (--skip-build flag)${NC}"
        echo ""
        return
    fi

    echo -e "${BLUE}[3/8] Building project (UI + Backend)...${NC}"
    echo ""

    # Check if Makefile exists
    if [ ! -f "Makefile" ]; then
        echo -e "${RED}Error: Makefile not found${NC}"
        echo "Please run this script from the project root directory"
        exit 1
    fi

    # Build
    if [ "$BUILD_MODE" = "release" ]; then
        echo "Building release binaries..."
        make release
    else
        echo "Building debug binaries..."
        make build
    fi

    # Check if binary was created
    if [ ! -f "$BINARY_SOURCE" ]; then
        echo -e "${RED}Error: Build failed, binary not found at $BINARY_SOURCE${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓ Build completed: $BINARY_SOURCE${NC}"
    echo ""
}

#############################################################################
# 4. Create directories
#############################################################################
create_directories() {
    echo -e "${BLUE}[4/8] Creating directories...${NC}"

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
# 5. Install binary
#############################################################################
install_binary() {
    echo -e "${BLUE}[5/8] Installing binary...${NC}"

    if [ ! -f "$BINARY_SOURCE" ]; then
        echo -e "${RED}Error: Binary not found at $BINARY_SOURCE${NC}"
        echo "Build the project first: make release"
        exit 1
    fi

    # Copy binary
    cp "$BINARY_SOURCE" "$INSTALL_DIR/$SERVICE_NAME"
    chmod +x "$INSTALL_DIR/$SERVICE_NAME"

    echo -e "${GREEN}✓ Binary installed: $INSTALL_DIR/$SERVICE_NAME${NC}"
    echo ""
}

#############################################################################
# 6. Install/Update configuration
#############################################################################
install_config() {
    echo -e "${BLUE}[6/8] Installing configuration...${NC}"

    local config_file="$CONFIG_DIR/config.toml"

    if [ -f "$CONFIG_SOURCE" ]; then
        # Backup existing config
        if [ -f "$config_file" ]; then
            cp "$config_file" "${config_file}.backup.$(date +%Y%m%d%H%M%S)"
            echo -e "${YELLOW}⊙ Backed up existing config${NC}"
        fi

        # Copy new config
        cp "$CONFIG_SOURCE" "$config_file"
        echo -e "${GREEN}✓ Configuration installed: $config_file${NC}"
    else
        # Create minimal config
        cat > "$config_file" << EOF
[server]
host = "0.00.0.0"
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
# 7. Create systemd service
#############################################################################
create_service() {
    echo -e "${BLUE}[7/8] Creating systemd service...${NC}"

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

    # Reload systemd
    systemctl daemon-reload

    echo -e "${GREEN}✓ Systemd service created: $service_file${NC}"
    echo ""
}

#############################################################################
# 8. Start service
#############################################################################
start_service() {
    echo -e "${BLUE}[8/8] Starting service...${NC}"

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
# Show deployment summary
#############################################################################
show_summary() {
    echo ""
    echo -e "${GREEN}=== Deployment Complete ===${NC}"
    echo ""
    echo -e "${BLUE}Installation Locations:${NC}"
    echo "  Binary:   $INSTALL_DIR/$SERVICE_NAME"
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
    echo "  URL:      http://localhost:3373"
    echo "  API Docs: http://localhost:3373/swagger-ui/"
    echo ""
    echo -e "${YELLOW}Next Steps:${NC}"
    echo "  1. Configure SSH access to cluster nodes:"
    echo "     sudo ./scripts/setup-ssh.sh"
    echo ""
    echo "  2. Add nodes via Web UI or API"
    echo ""
    echo "  3. Create DRBD resources and HA profiles"
    echo ""
}

#############################################################################
# Main execution
#############################################################################
main() {
    check_root
    check_dependencies
    build_project
    create_directories
    install_binary
    install_config
    create_service
    start_service
    show_summary
}

# Run main function
main "$@"
