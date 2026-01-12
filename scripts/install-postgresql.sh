#!/bin/bash

# PostgreSQL Installation Script
# Install PostgreSQL server in parallel on all HA nodes
#
# Usage: ./install-postgresql.sh [node_list]
# Example: ./install-postgresql.sh orange1,orange2,orange3
#          ./install-postgresql.sh node1,node2

# Default node list
DEFAULT_NODES="orange1,orange2,orange3"

# Parse command line arguments
NODES_ARG="${1:-$DEFAULT_NODES}"

# Convert comma-separated string to array
IFS=',' read -ra NODES <<< "$NODES_ARG"

# Color output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}PostgreSQL Parallel Installation Script${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${BLUE}Target nodes: ${NODES[*]}${NC}"
echo ""

# Array to store background process PIDs
PIDS=()

# Iterate through all nodes, start parallel installation
for node in "${NODES[@]}"; do
    echo -e "${BLUE}Starting installation process: $node${NC}"

    # Execute SSH installation in background
    (
        echo -e "${YELLOW}[$node] Starting installation...${NC}"
        ssh "$node" "sudo apt install -y postgresql" 2>&1 | while IFS= read -r line; do
            echo -e "${YELLOW}[$node]${NC} $line"
        done

        if [ ${PIPESTATUS[0]} -eq 0 ]; then
            echo -e "${GREEN}[$node] ✓ Installation successful${NC}"
        else
            echo -e "${RED}[$node] ✗ Installation failed${NC}"
        fi
    ) &

    # Record background process PID
    PIDS+=($!)
done

echo ""
echo -e "${BLUE}All installation processes started, waiting for completion...${NC}"
echo ""

# Wait for all background processes to complete
for pid in "${PIDS[@]}"; do
    wait $pid
done

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}All installation tasks completed!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "You can check PostgreSQL service status with the following commands:"
for node in "${NODES[@]}"; do
    echo "  ssh $node 'sudo systemctl status postgresql'"
done
