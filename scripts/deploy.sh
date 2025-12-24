#!/bin/bash
set -e

#############################################################################
# DRBD HA Manager - Deployment Script
#
# This script builds the project locally and deploys to a remote server.
#
# Usage: ./scripts/deploy.sh <user@host> [OPTIONS]
#
# Examples:
#   ./scripts/deploy.sh root@192.168.1.100
#   ./scripts/deploy.sh root@orange1 --skip-build
#   ./scripts/deploy.sh root@orange1 --dev
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
RA_PARAMS_SOURCE="./target/release/ra-params"
CONFIG_SOURCE="./drbd-ha/config/default.toml"
INSTALL_SCRIPT_SOURCE="./scripts/install.sh"

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
            RA_PARAMS_SOURCE="./target/debug/ra-params"
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
    echo "  --skip-build       Skip building, deploy existing binaries"
    echo "  --dev              Build in debug mode"
    echo ""
    echo "Examples:"
    echo "  $0 root@orange1                    # Build and deploy"
    echo "  $0 root@orange1 --skip-build       # Deploy existing binaries"
    echo "  $0 root@192.168.1.100 --dev        # Debug build"
    exit 1
fi

echo -e "${BLUE}=== DRBD HA Manager - Remote Deployment ===${NC}"
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
# 4. Deploy files to remote
#############################################################################
deploy_files() {
    echo -e "${BLUE}[4/5] Deploying files to remote...${NC}"

    # Create temporary directory on remote
    echo "Creating temporary directory on $REMOTE_HOST..."
    ssh "$REMOTE_HOST" "mkdir -p /tmp/drbd-ha-deploy"

    # Copy binary
    echo "Copying binary to $REMOTE_HOST:/tmp/drbd-ha-deploy/..."
    scp "$BINARY_SOURCE" "$REMOTE_HOST:/tmp/drbd-ha-deploy/drbd-ha"
    echo -e "${GREEN}✓ Binary deployed${NC}"

    # Copy ra-params
    if [ -f "$RA_PARAMS_SOURCE" ]; then
        echo "Copying ra-params to $REMOTE_HOST:/tmp/drbd-ha-deploy/..."
        scp "$RA_PARAMS_SOURCE" "$REMOTE_HOST:/tmp/drbd-ha-deploy/ra-params"
        echo -e "${GREEN}✓ ra-params deployed${NC}"
    else
        echo -e "${YELLOW}⊙ ra-params not found at $RA_PARAMS_SOURCE${NC}"
    fi

    # Copy config
    if [ -f "$CONFIG_SOURCE" ]; then
        echo "Copying configuration to $REMOTE_HOST:/tmp/drbd-ha-deploy/..."
        scp "$CONFIG_SOURCE" "$REMOTE_HOST:/tmp/drbd-ha-deploy/config.toml"
        echo -e "${GREEN}✓ Configuration deployed${NC}"
    else
        echo -e "${YELLOW}⊙ Config file not found, will use default on remote${NC}"
    fi

    # Copy install script
    echo "Copying install script to $REMOTE_HOST:/tmp/drbd-ha-deploy/..."
    scp "$INSTALL_SCRIPT_SOURCE" "$REMOTE_HOST:/tmp/drbd-ha-deploy/install.sh"
    ssh "$REMOTE_HOST" "chmod +x /tmp/drbd-ha-deploy/install.sh"
    echo -e "${GREEN}✓ Install script deployed${NC}"

    echo ""
}

#############################################################################
# 5. Run install script on remote
#############################################################################
run_remote_install() {
    echo -e "${BLUE}[5/5] Running install script on remote...${NC}"
    echo ""
    echo "Executing: ssh $REMOTE_HOST 'sudo /tmp/drbd-ha-deploy/install.sh'"
    echo ""

    ssh "$REMOTE_HOST" "sudo /tmp/drbd-ha-deploy/install.sh"

    # Cleanup
    echo ""
    echo "Cleaning up temporary files..."
    ssh "$REMOTE_HOST" "rm -rf /tmp/drbd-ha-deploy"
    echo -e "${GREEN}✓ Cleanup complete${NC}"
}

#############################################################################
# Show deployment summary
#############################################################################
show_summary() {
    echo ""
    echo -e "${GREEN}=== Remote Deployment Complete ===${NC}"
    echo ""
    echo -e "${BLUE}Target:${NC} $REMOTE_HOST"
    echo ""
    echo -e "${BLUE}Remote Locations:${NC}"
    echo "  Binary:   /opt/drbd-ha/drbd-ha"
    echo "  Config:   /etc/drbd-ha/config.toml"
    echo "  Logs:     /var/log/drbd-ha/drbd-ha.log"
    echo ""
    echo -e "${BLUE}Remote Service Commands:${NC}"
    echo "  ssh $REMOTE_HOST 'systemctl start drbd-ha'"
    echo "  ssh $REMOTE_HOST 'systemctl stop drbd-ha'"
    echo "  ssh $REMOTE_HOST 'systemctl restart drbd-ha'"
    echo "  ssh $REMOTE_HOST 'systemctl status drbd-ha'"
    echo "  ssh $REMOTE_HOST 'journalctl -u drbd-ha -f'"
    echo ""
    echo -e "${BLUE}Web Interface:${NC}"
    # Extract IP from user@host format
    local ip=$(echo "$REMOTE_HOST" | cut -d@ -f2)
    echo "  URL:      http://$ip:3373"
    echo "  API Docs: http://$ip:3373/swagger-ui/"
    echo ""
    echo -e "${YELLOW}Next Steps:${NC}"
    echo "  1. Configure SSH keys for root user on $REMOTE_HOST:"
    echo "     ssh $REMOTE_HOST"
    echo "     sudo -i"
    echo "     ssh-keygen -t rsa -b 4096"
    echo "     ssh-copy-id <user>@<node>"
    echo "     ssh -o BatchMode=yes <user>@<node> echo ok  # Test"
    echo ""
    echo "     IMPORTANT: drbd-ha runs as root, so configure keys for /root/.ssh/"
    echo "  2. Add nodes via Web UI or API"
    echo "  3. Create DRBD resources and HA profiles"
    echo ""
}

#############################################################################
# Main execution
#############################################################################
main() {
    check_local_prerequisites
    build_project
    test_ssh_connection
    deploy_files
    run_remote_install
    show_summary
}

# Run main function
main "$@"
