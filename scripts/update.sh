#!/bin/bash
set -e

#############################################################################
# DRBD HA Manager - Remote Update Script
#
# This script builds locally and updates a remote server.
# Use this to update an existing deployment without reinstalling.
#
# Usage: ./scripts/update.sh <user@host> [OPTIONS]
#
# Examples:
#   ./scripts/update.sh root@orange1
#   ./scripts/update.sh root@orange1 --skip-build
#############################################################################

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
REMOTE_HOST=""
BINARY_SOURCE="./target/release/drbd-ha"
CONFIG_SOURCE="./drbd-ha/config/default.toml"
SERVICE_NAME="drbd-ha"
INSTALL_DIR="/opt/drbd-ha"

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
        -*)
            echo "Unknown option: $arg"
            echo "Usage: $0 <user@host> [--skip-build] [--dev]"
            exit 1
            ;;
        *)
            if [[ -z "$REMOTE_HOST" ]]; then
                REMOTE_HOST="$arg"
            fi
            shift
            ;;
    esac
done

# Check if remote host is provided
if [[ -z "$REMOTE_HOST" ]]; then
    echo -e "${RED}Error: Remote host not specified${NC}"
    echo ""
    echo "Usage: $0 <user@host> [OPTIONS]"
    echo ""
    echo "Arguments:"
    echo "  user@host          Remote server (e.g., root@orange1, root@192.168.1.100)"
    echo ""
    echo "Options:"
    echo "  --skip-build       Skip building, update with existing binary"
    echo "  --dev              Build in debug mode"
    echo ""
    echo "Examples:"
    echo "  $0 root@orange1                    # Build and update"
    echo "  $0 root@orange1 --skip-build       # Update existing binary"
    echo "  $0 root@192.168.1.100 --dev        # Debug build"
    exit 1
fi

echo -e "${BLUE}=== DRBD HA Manager - Remote Update ===${NC}"
echo -e "Target: ${GREEN}$REMOTE_HOST${NC}"
echo ""

#############################################################################
# 1. Check local prerequisites
#############################################################################
check_local_prerequisites() {
    echo -e "${BLUE}[1/5] Checking local prerequisites...${NC}"

    # Check if Makefile exists
    if [ ! -f "Makefile" ]; then
        echo -e "${RED}Error: Makefile not found${NC}"
        echo "Please run this script from the project root directory"
        exit 1
    fi

    # Check for scp
    if ! command -v scp &> /dev/null; then
        echo -e "${RED}Error: scp not found${NC}"
        echo "Install it: sudo apt-get install openssh-client"
        exit 1
    fi

    # Check for ssh
    if ! command -v ssh &> /dev/null; then
        echo -e "${RED}Error: ssh not found${NC}"
        echo "Install it: sudo apt-get install openssh-client"
        exit 1
    fi

    echo -e "${GREEN}✓ Local prerequisites OK${NC}"
    echo ""
}

#############################################################################
# 2. Build project locally
#############################################################################
build_project() {
    if [ "$SKIP_BUILD" = true ]; then
        echo -e "${BLUE}[2/5] Building project...${NC}"
        echo -e "${YELLOW}⊙ Skipping build (--skip-build flag)${NC}"

        # Check if binary exists
        if [ ! -f "$BINARY_SOURCE" ]; then
            echo -e "${RED}Error: Binary not found at $BINARY_SOURCE${NC}"
            echo "Build the project first: make release"
            exit 1
        fi
        echo -e "${GREEN}✓ Using existing binary: $BINARY_SOURCE${NC}"
        echo ""
        return
    fi

    echo -e "${BLUE}[2/5] Building project locally (UI + Backend)...${NC}"
    echo ""

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
# 3. Test SSH connection
#############################################################################
test_ssh_connection() {
    echo -e "${BLUE}[3/5] Testing SSH connection...${NC}"

    echo "Testing connection to $REMOTE_HOST..."
    if ssh -o ConnectTimeout=5 -o BatchMode=yes "$REMOTE_HOST" "echo ok" >/dev/null 2>&1; then
        echo -e "${GREEN}✓ SSH connection successful${NC}"
    else
        echo -e "${YELLOW}⚠ SSH connection test failed${NC}"
        echo "  You may need to:"
        echo "  1. Setup SSH keys: ssh-copy-id $REMOTE_HOST"
        echo "  2. Or enter password when prompted"
        echo ""
        read -p "Continue anyway? (y/N): " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo "Aborted."
            exit 1
        fi
    fi

    echo ""
}

#############################################################################
# 4. Stop remote service and upload binary
#############################################################################
update_remote_service() {
    echo -e "${BLUE}[4/5] Updating remote service...${NC}"

    # Stop the service
    echo "Stopping service on $REMOTE_HOST..."
    ssh "$REMOTE_HOST" "sudo systemctl stop $SERVICE_NAME" || true
    echo -e "${GREEN}✓ Service stopped${NC}"

    # Clean up old temp directory (if exists) and create new one
    # Create as root then chown to allow scp to write
    ssh "$REMOTE_HOST" "sudo rm -rf /tmp/drbd-ha-update && sudo mkdir -p /tmp/drbd-ha-update && sudo chown \$(whoami):\$(whoami) /tmp/drbd-ha-update"

    # Copy new binary
    echo "Copying new binary to $REMOTE_HOST..."
    scp "$BINARY_SOURCE" "$REMOTE_HOST:/tmp/drbd-ha-update/$SERVICE_NAME"
    echo -e "${GREEN}✓ Binary uploaded${NC}"

    # Copy config if exists
    if [ -f "$CONFIG_SOURCE" ]; then
        echo "Copying configuration to $REMOTE_HOST..."
        scp "$CONFIG_SOURCE" "$REMOTE_HOST:/tmp/drbd-ha-update/config.toml"

        # Backup existing config
        ssh "$REMOTE_HOST" "
            if [ -f /etc/drbd-ha/config.toml ]; then
                sudo cp /etc/drbd-ha/config.toml /etc/drbd-ha/config.toml.backup.\$(date +%Y%m%d%H%M%S)
                echo '  Backed up existing config'
            fi
        "

        echo -e "${GREEN}✓ Configuration uploaded${NC}"
    fi

    # Install new binary
    echo "Installing new binary..."
    ssh "$REMOTE_HOST" "
        sudo cp /tmp/drbd-ha-update/$SERVICE_NAME $INSTALL_DIR/$SERVICE_NAME
        sudo chmod +x $INSTALL_DIR/$SERVICE_NAME
    "
    echo -e "${GREEN}✓ Binary installed${NC}"

    # Update config if provided
    if [ -f "$CONFIG_SOURCE" ]; then
        ssh "$REMOTE_HOST" "sudo cp /tmp/drbd-ha-update/config.toml /etc/drbd-ha/config.toml"
        echo -e "${GREEN}✓ Configuration updated${NC}"
    fi

    # Clean up temp directory
    ssh "$REMOTE_HOST" "sudo rm -rf /tmp/drbd-ha-update"

    echo ""
}

#############################################################################
# 5. Start remote service
#############################################################################
start_remote_service() {
    echo -e "${BLUE}[5/5] Starting remote service...${NC}"

    echo "Starting service on $REMOTE_HOST..."
    ssh "$REMOTE_HOST" "
        sudo systemctl daemon-reload
        sudo systemctl start $SERVICE_NAME
        sleep 2
    "

    # Check status
    echo ""
    if ssh "$REMOTE_HOST" "sudo systemctl is-active --quiet $SERVICE_NAME"; then
        echo -e "${GREEN}✓ Service updated and started successfully on $REMOTE_HOST${NC}"
        echo ""
        echo "Remote service status:"
        ssh "$REMOTE_HOST" "sudo systemctl status $SERVICE_NAME --no-pager -l"
    else
        echo -e "${RED}✗ Failed to start service on $REMOTE_HOST${NC}"
        echo ""
        echo "Check remote logs:"
        echo "  ssh $REMOTE_HOST 'sudo journalctl -u $SERVICE_NAME -n 50'"
        echo "  ssh $REMOTE_HOST 'sudo cat /var/log/drbd-ha/drbd-ha.log'"
        exit 1
    fi

    echo ""
}

#############################################################################
# Show deployment summary
#############################################################################
show_summary() {
    echo ""
    echo -e "${GREEN}=== Remote Update Complete ===${NC}"
    echo ""
    echo -e "${BLUE}Target:${NC} $REMOTE_HOST"
    echo ""
    echo -e "${BLUE}Remote Locations:${NC}"
    echo "  Binary:   $INSTALL_DIR/$SERVICE_NAME"
    echo "  Config:   /etc/drbd-ha/config.toml"
    echo "  Logs:     /var/log/drbd-ha/drbd-ha.log"
    echo ""
    echo -e "${BLUE}Remote Service Commands:${NC}"
    echo "  ssh $REMOTE_HOST 'sudo systemctl start $SERVICE_NAME'"
    echo "  ssh $REMOTE_HOST 'sudo systemctl stop $SERVICE_NAME'"
    echo "  ssh $REMOTE_HOST 'sudo systemctl restart $SERVICE_NAME'"
    echo "  ssh $REMOTE_HOST 'sudo systemctl status $SERVICE_NAME'"
    echo "  ssh $REMOTE_HOST 'sudo journalctl -u $SERVICE_NAME -f'"
    echo ""
    echo -e "${BLUE}Web Interface:${NC}"
    # Extract IP from user@host format
    local ip=$(echo "$REMOTE_HOST" | cut -d@ -f2)
    echo "  URL:      http://$ip:3373"
    echo "  API Docs: http://$ip:3373/swagger-ui/"
    echo ""
}

#############################################################################
# Main execution
#############################################################################
main() {
    check_local_prerequisites
    build_project
    test_ssh_connection
    update_remote_service
    start_remote_service
    show_summary
}

# Run main function
main "$@"
