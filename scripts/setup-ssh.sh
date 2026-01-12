#!/bin/bash
# SSH Setup Helper for DRBD-HA
# This script helps configure passwordless SSH access for DRBD-HA

set -e

SCRIPT_NAME=$(basename "$0")

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

function print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

function print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

function print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

function show_usage() {
    cat << EOF
Usage: $SCRIPT_NAME [OPTIONS] node1 node2 node3 ...

Configure passwordless SSH access for DRBD-HA orchestration.

OPTIONS:
    -u, --user USER      SSH username (default: root)
    -k, --key FILE       SSH key file (default: ~/.ssh/id_drbd_ha)
    -p, --port PORT      SSH port (default: 22)
    -h, --help           Show this help message

EXAMPLES:
    # Setup SSH for root user on 3 nodes
    $SCRIPT_NAME node1 node2 node3

    # Setup with custom user
    $SCRIPT_NAME -u ubuntu node1 node2 node3

    # Use existing key
    $SCRIPT_NAME -k ~/.ssh/my_key node1 node2

    # Setup with IP addresses
    $SCRIPT_NAME 192.168.1.101 192.168.1.102 192.168.1.103

EOF
}

# Default values
SSH_USER="root"
SSH_KEY="$HOME/.ssh/id_drbd_ha"
SSH_PORT=22
NODES=()

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -u|--user)
            SSH_USER="$2"
            shift 2
            ;;
        -k|--key)
            SSH_KEY="$2"
            shift 2
            ;;
        -p|--port)
            SSH_PORT="$2"
            shift 2
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        *)
            NODES+=("$1")
            shift
            ;;
    esac
done

if [[ ${#NODES[@]} -eq 0 ]]; then
    print_error "No nodes specified"
    show_usage
    exit 1
fi

print_info "=== DRBD-HA SSH Setup ==="
print_info "User: $SSH_USER"
print_info "Key: $SSH_KEY"
print_info "Port: $SSH_PORT"
print_info "Nodes: ${NODES[*]}"
echo ""

# 1. Generate SSH key if it doesn't exist
if [[ ! -f "$SSH_KEY" ]]; then
    print_info "SSH key not found, generating new key..."
    ssh-keygen -t ed25519 -f "$SSH_KEY" -N "" -C "drbd-ha-controller"
    print_info "Created SSH key: $SSH_KEY"
else
    print_info "Using existing SSH key: $SSH_KEY"
fi

# Ensure proper permissions
chmod 600 "$SSH_KEY"
chmod 644 "${SSH_KEY}.pub"

echo ""
print_info "=== Distributing SSH Keys ==="

# 2. Distribute keys to all nodes
SUCCESS_NODES=()
FAILED_NODES=()

for node in "${NODES[@]}"; do
    print_info "Setting up $node..."
    
    # Try to copy SSH key
    if ssh-copy-id -i "$SSH_KEY.pub" -p "$SSH_PORT" "${SSH_USER}@${node}" 2>/dev/null; then
        print_info "✓ Key copied to $node"
    else
        print_warn "SSH key already exists or copy failed for $node (this might be OK)"
    fi
    
    # Test connection
    if ssh -i "$SSH_KEY" -p "$SSH_PORT" -o ConnectTimeout=5 -o StrictHostKeyChecking=no \
        "${SSH_USER}@${node}" "echo OK" >/dev/null 2>&1; then
        print_info "✓ Connection test passed: $node"
        SUCCESS_NODES+=("$node")
    else
        print_error "✗ Connection test failed: $node"
        FAILED_NODES+=("$node")
    fi
done

echo ""
print_info "=== Testing Sudo Access ==="

# 3. Test sudo access on each node
SUDO_OK=()
SUDO_FAILED=()

for node in "${SUCCESS_NODES[@]}"; do
    print_info "Testing sudo on $node..."
    
    if ssh -i "$SSH_KEY" -p "$SSH_PORT" "${SSH_USER}@${node}" \
        "sudo -n id" >/dev/null 2>&1; then
        print_info "✓ Sudo (passwordless) works on $node"
        SUDO_OK+=("$node")
    else
        print_warn "✗ Sudo requires password on $node"
        print_warn "  Run on $node: sudo visudo"
        print_warn "  Add line: ${SSH_USER} ALL=(ALL) NOPASSWD:ALL"
        SUDO_FAILED+=("$node")
    fi
done

# 4. Summary
echo ""
echo "========================================"
echo "           Setup Summary"
echo "========================================"
echo ""

if [[ ${#SUCCESS_NODES[@]} -gt 0 ]]; then
    print_info "✓ SSH connection successful (${#SUCCESS_NODES[@]} nodes):"
    for node in "${SUCCESS_NODES[@]}"; do
        echo "    - $node"
    done
fi

if [[ ${#FAILED_NODES[@]} -gt 0 ]]; then
    print_error "✗ SSH connection failed (${#FAILED_NODES[@]} nodes):"
    for node in "${FAILED_NODES[@]}"; do
        echo "    - $node"
    done
fi

echo ""

if [[ ${#SUDO_OK[@]} -gt 0 ]]; then
    print_info "✓ Passwordless sudo OK (${#SUDO_OK[@]} nodes):"
    for node in "${SUDO_OK[@]}"; do
        echo "    - $node"
    done
fi

if [[ ${#SUDO_FAILED[@]} -gt 0 ]]; then
    print_warn "⚠ Sudo needs configuration (${#SUDO_FAILED[@]} nodes):"
    for node in "${SUDO_FAILED[@]}"; do
        echo "    - $node"
    done
    echo ""
    print_warn "To fix, run on each node:"
    print_warn "  sudo visudo"
    print_warn "  Add: ${SSH_USER} ALL=(ALL) NOPASSWD:ALL"
fi

echo ""
print_info "SSH Key Location: $SSH_KEY"
print_info "Use this path when adding nodes in DRBD-HA Web UI"
echo ""

# 5. Create a config snippet
CONFIG_SNIPPET="/tmp/drbd-ha-ssh-config.txt"
cat > "$CONFIG_SNIPPET" << EOF
# DRBD-HA Configuration Snippet
# Generated: $(date)

[ssh]
default_user = "$SSH_USER"
default_port = $SSH_PORT

# When adding nodes in Web UI, use:
# SSH Private Key Path: $SSH_KEY

# Nodes ready for management:
EOF

for node in "${SUCCESS_NODES[@]}"; do
    echo "#   - ${SSH_USER}@${node}" >> "$CONFIG_SNIPPET"
done

print_info "Configuration snippet saved to: $CONFIG_SNIPPET"

if [[ ${#FAILED_NODES[@]} -gt 0 ]] || [[ ${#SUDO_FAILED[@]} -gt 0 ]]; then
    exit 1
fi

exit 0
