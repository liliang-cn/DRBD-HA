#!/bin/bash
set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}=== DRBD HA Manager - SSH Setup Helper ===${NC}"
echo ""
echo -e "${YELLOW}Purpose:${NC}"
echo "  This script configures passwordless SSH access from the manager node to cluster nodes."
echo "  The DRBD HA service uses system SSH (not password-based auth) to manage remote nodes."
echo ""
echo -e "${YELLOW}Prerequisites:${NC}"
echo "  1. Run this script as the same user who runs drbd-ha service (usually root)"
echo "  2. Know the password for remote user on each node"
echo "  3. Network connectivity to all cluster nodes"
echo "  4. Remote user should have sudo privileges (for LVM/DRBD operations)"
echo ""
echo -e "${YELLOW}What this script does:${NC}"
echo "  1. Generates SSH key pair if not exists (~/.ssh/id_rsa)"
echo "  2. Copies public key to remote nodes (using ssh-copy-id)"
echo "  3. Verifies passwordless login to all nodes"
echo ""

# Ask for remote username
echo "Enter the SSH username for remote nodes (default: root):"
read -r REMOTE_USER
REMOTE_USER=${REMOTE_USER:-root}
echo -e "Using username: ${GREEN}$REMOTE_USER${NC}"
echo ""

echo "Press Enter to continue, or Ctrl+C to cancel..."
read -r

# 1. Check/Generate SSH Key
SSH_DIR="$HOME/.ssh"
KEY_FILE="$SSH_DIR/id_rsa"

if [ ! -f "$KEY_FILE" ]; then
    echo "SSH key not found. Generating..."
    mkdir -p "$SSH_DIR"
    chmod 700 "$SSH_DIR"
    ssh-keygen -t rsa -b 4096 -f "$KEY_FILE" -N ""
    echo -e "${GREEN}Generated SSH key at $KEY_FILE${NC}"
else
    echo -e "${GREEN}Found existing SSH key at $KEY_FILE${NC}"
fi

# 2. Add Nodes
echo ""
echo "Enter the IP addresses or hostnames of your cluster nodes (space separated):"
read -r NODES

if [ -z "$NODES" ]; then
    echo "No nodes provided. Exiting."
    exit 0
fi

for NODE in $NODES; do
    echo ""
    echo -e "Processing node: ${GREEN}$NODE${NC}"
    
    # Check if we can already login
    if ssh -o BatchMode=yes -o StrictHostKeyChecking=no -o ConnectTimeout=5 "$REMOTE_USER@$NODE" "echo ok" >/dev/null 2>&1; then
        echo -e "${GREEN}Success: Passwordless access already exists for $REMOTE_USER@$NODE${NC}"
        continue
    fi

    echo "Copying ID to $REMOTE_USER@$NODE (you will be asked for password)..."
    if ssh-copy-id -o StrictHostKeyChecking=no "$REMOTE_USER@$NODE"; then
        echo -e "${GREEN}Success: Key copied to $REMOTE_USER@$NODE${NC}"
        
        # Test sudo access if not root
        if [ "$REMOTE_USER" != "root" ]; then
            echo -n "Testing sudo access... "
            if ssh -o BatchMode=yes -o StrictHostKeyChecking=no "$REMOTE_USER@$NODE" "sudo -n true" >/dev/null 2>&1; then
                echo -e "${GREEN}OK (passwordless sudo)${NC}"
            else
                echo -e "${YELLOW}WARNING: User may need to enter sudo password${NC}"
                echo "  For best experience, configure passwordless sudo on $NODE:"
                echo "  sudo visudo -f /etc/sudoers.d/$REMOTE_USER"
                echo "  Add: $REMOTE_USER ALL=(ALL) NOPASSWD:ALL"
            fi
        fi
    else
        echo -e "${RED}Error: Failed to copy key to $REMOTE_USER@$NODE${NC}"
    fi
done

echo ""
echo -e "${GREEN}=== Setup Complete ===${NC}"
echo ""
echo "Verification Results:"
echo "--------------------"
for NODE in $NODES; do
    echo -n "$NODE ($REMOTE_USER) ... "
    if ssh -o BatchMode=yes -o StrictHostKeyChecking=no -o ConnectTimeout=5 "$REMOTE_USER@$NODE" "echo ok" >/dev/null 2>&1; then
        echo -e "${GREEN}✓ SSH OK${NC}"
        
        # Check sudo if not root
        if [ "$REMOTE_USER" != "root" ]; then
            echo -n "  Sudo check ... "
            if ssh -o BatchMode=yes -o StrictHostKeyChecking=no "$REMOTE_USER@$NODE" "sudo -n true" >/dev/null 2>&1; then
                echo -e "${GREEN}✓ Passwordless sudo${NC}"
            else
                echo -e "${YELLOW}⚠ Sudo may require password${NC}"
            fi
        fi
    else
        echo -e "${RED}✗ FAILED${NC}"
        echo "  Troubleshooting:"
        echo "    - Check if node is reachable: ping $NODE"
        echo "    - Try manual SSH: ssh $REMOTE_USER@$NODE"
        echo "    - Re-run ssh-copy-id: ssh-copy-id $REMOTE_USER@$NODE"
    fi
done

echo ""
echo -e "${YELLOW}Next Steps:${NC}"
echo "  1. Update DRBD HA config if using non-root user:"
echo "     File: /etc/drbd-ha/config.toml"
echo "     Set: [ssh] default_user = \"$REMOTE_USER\""
echo ""
echo "  2. Add nodes to DRBD HA Manager via Web UI or API"
echo "     - Use '$REMOTE_USER' as SSH username when adding nodes"
echo ""
echo "  3. Nodes should show status 'Online' if SSH is properly configured"
echo ""
echo "  4. Test SSH access manually:"
echo "     ssh -o BatchMode=yes $REMOTE_USER@<node-ip> 'echo ok'"
echo ""
