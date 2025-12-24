#!/bin/bash
set -e

#############################################################################
# DRBD HA Manager - Uninstall Script
#
# This script removes the drbd-ha service and all its components.
# It will NOT remove DRBD resources, LVM volumes, or user data.
#
# Usage: sudo ./scripts/uninstall.sh [--purge-all]
#############################################################################

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
INSTALL_DIR="/opt/drbd-ha"
CONFIG_DIR="/etc/drbd-ha"
DATA_DIR="/var/lib/drbd-ha"
LOG_DIR="/var/log/drbd-ha"
SERVICE_NAME="drbd-ha"
PURGE_ALL=false

# Parse arguments
for arg in "$@"; do
    case $arg in
        --purge-all)
            PURGE_ALL=true
            shift
            ;;
        *)
            ;;
    esac
done

echo -e "${BLUE}=== DRBD HA Manager - Uninstall Script ===${NC}"
echo ""

#############################################################################
# Warning
#############################################################################
show_warning() {
    echo -e "${RED}WARNING: This will uninstall the DRBD HA Manager service!${NC}"
    echo ""
    echo "This will:"
    echo "  - Stop and disable the $SERVICE_NAME service"
    echo "  - Remove the systemd service file"
    echo "  - Remove the binary from $INSTALL_DIR"
    if [ "$PURGE_ALL" = true ]; then
        echo -e "${RED}  - DELETE ALL CONFIGURATION in $CONFIG_DIR${NC}"
        echo -e "${RED}  - DELETE ALL DATA in $DATA_DIR${NC}"
        echo -e "${RED}  - DELETE ALL LOGS in $LOG_DIR${NC}"
    else
        echo "  - Keep configuration, data, and logs"
    fi
    echo ""
    echo "This will NOT:"
    echo "  - Remove DRBD resources"
    echo "  - Remove LVM volumes or ZFS pools"
    echo "  - Affect running DRBD connections"
    echo ""
    echo -n "Type 'yes' to continue: "
    read -r response
    if [ "$response" != "yes" ]; then
        echo "Aborted."
        exit 0
    fi
    echo ""
}

#############################################################################
# 1. Stop and disable service
#############################################################################
stop_service() {
    echo -e "${BLUE}[1/5] Stopping service...${NC}"

    if systemctl is-active --quiet "$SERVICE_NAME"; then
        echo "Stopping $SERVICE_NAME..."
        systemctl stop "$SERVICE_NAME"
        echo -e "${GREEN}✓ Service stopped${NC}"
    else
        echo -e "${YELLOW}⊙ Service not running${NC}"
    fi

    if systemctl is-enabled --quiet "$SERVICE_NAME"; then
        echo "Disabling $SERVICE_NAME..."
        systemctl disable "$SERVICE_NAME"
        echo -e "${GREEN}✓ Service disabled${NC}"
    else
        echo -e "${YELLOW}⊙ Service not enabled${NC}"
    fi

    echo ""
}

#############################################################################
# 2. Remove systemd service file
#############################################################################
remove_service_file() {
    echo -e "${BLUE}[2/5] Removing systemd service...${NC}"

    local service_file="/etc/systemd/system/${SERVICE_NAME}.service"

    if [ -f "$service_file" ]; then
        rm -f "$service_file"
        systemctl daemon-reload
        systemctl reset-failed
        echo -e "${GREEN}✓ Service file removed${NC}"
    else
        echo -e "${YELLOW}⊙ Service file not found${NC}"
    fi

    echo ""
}

#############################################################################
# 3. Remove binary
#############################################################################
remove_binary() {
    echo -e "${BLUE}[3/5] Removing binary...${NC}"

    if [ -f "$INSTALL_DIR/$SERVICE_NAME" ]; then
        rm -f "$INSTALL_DIR/$SERVICE_NAME"
        echo -e "${GREEN}✓ Binary removed${NC}"
    else
        echo -e "${YELLOW}⊙ Binary not found${NC}"
    fi

    # Try to remove install directory if empty
    if [ -d "$INSTALL_DIR" ]; then
        rmdir --ignore-fail-on-non-empty "$INSTALL_DIR" 2>/dev/null || true
        if [ ! -d "$INSTALL_DIR" ]; then
            echo -e "${GREEN}✓ Install directory removed${NC}"
        fi
    fi

    echo ""
}

#############################################################################
# 4. Remove configuration and data
#############################################################################
remove_config_data() {
    echo -e "${BLUE}[4/5] Removing configuration and data...${NC}"

    if [ "$PURGE_ALL" = true ]; then
        # Remove everything
        if [ -d "$CONFIG_DIR" ]; then
            echo -e "${RED}Deleting configuration...${NC}"
            rm -rf "$CONFIG_DIR"
            echo -e "${GREEN}✓ Configuration directory removed${NC}"
        fi

        if [ -d "$DATA_DIR" ]; then
            echo -e "${RED}Deleting data...${NC}"
            rm -rf "$DATA_DIR"
            echo -e "${GREEN}✓ Data directory removed${NC}"
        fi

        if [ -d "$LOG_DIR" ]; then
            echo -e "${RED}Deleting logs...${NC}"
            rm -rf "$LOG_DIR"
            echo -e "${GREEN}✓ Log directory removed${NC}"
        fi
    else
        # Keep files, just show where they are
        echo -e "${YELLOW}Keeping existing files:${NC}"
        [ -d "$CONFIG_DIR" ] && echo "  Config:  $CONFIG_DIR"
        [ -d "$DATA_DIR" ] && echo "  Data:    $DATA_DIR"
        [ -d "$LOG_DIR" ] && echo "  Logs:    $LOG_DIR"
        echo ""
        echo -e "${YELLOW}To remove them manually:${NC}"
        echo "  rm -rf $CONFIG_DIR"
        echo "  rm -rf $DATA_DIR"
        echo "  rm -rf $LOG_DIR"
    fi

    echo ""
}

#############################################################################
# 5. Show summary
#############################################################################
show_summary() {
    echo -e "${BLUE}[5/5] Cleanup summary${NC}"
    echo ""
    echo -e "${GREEN}✓ DRBD HA Manager uninstalled${NC}"
    echo ""

    if [ "$PURGE_ALL" = false ]; then
        echo -e "${YELLOW}Note: The following were preserved:${NC}"
        echo "  - Configuration: $CONFIG_DIR"
        echo "  - Data:         $DATA_DIR"
        echo "  - Logs:         $LOG_DIR"
        echo ""
        echo "To completely remove all files, run:"
        echo "  sudo ./scripts/uninstall.sh --purge-all"
        echo ""
    fi

    echo "To reinstall, run:"
    echo "  sudo ./scripts/deploy.sh"
    echo ""
}

#############################################################################
# Main execution
#############################################################################
main() {
    show_warning
    stop_service
    remove_service_file
    remove_binary
    remove_config_data
    show_summary
}

# Run main function
main "$@"
