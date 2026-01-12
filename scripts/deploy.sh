#!/bin/bash
set -e

# Single Host Deployment/Update Script
# Usage: ./scripts/deploy.sh <user@host> [--skip-build] [--restart]

if [[ $# -eq 0 ]]; then
    echo "Usage: $0 <user@host> [--skip-build] [--restart]"
    echo "Example: $0 ubuntu@orange1"
    echo "         $0 ubuntu@orange1 --skip-build --restart"
    echo ""
    echo "Options:"
    echo "  --skip-build    Don't rebuild, use existing binary"
    echo "  --restart       Restart drbd-ha service after deployment"
    exit 1
fi

REMOTE_HOST="$1"
shift
SKIP_BUILD=false
RESTART=false

# Parse options
while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --restart)
            RESTART=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "=== Deploying to $REMOTE_HOST ==="

# 1. Build (unless skipped)
if [[ "$SKIP_BUILD" == false ]]; then
    echo "[1/3] Building..."
    make release
else
    echo "[1/3] Skipping build (using existing binary)..."
fi

# 2. Deploy
echo "[2/3] Deploying to $REMOTE_HOST..."

# Create directories
ssh "$REMOTE_HOST" "sudo mkdir -p /opt/drbd-ha /etc/drbd-ha /var/log/drbd-ha"

# Copy binary
scp target/release/drbd-ha "$REMOTE_HOST:/tmp/drbd-ha"
ssh "$REMOTE_HOST" "sudo mv /tmp/drbd-ha /opt/drbd-ha/drbd-ha && sudo chmod +x /opt/drbd-ha/drbd-ha"
echo "  → Binary updated"

# Copy config if not exists
if ! ssh "$REMOTE_HOST" "test -f /etc/drbd-ha/config.toml"; then
    scp drbd-ha/config/default.toml "$REMOTE_HOST:/tmp/config.toml"
    ssh "$REMOTE_HOST" "sudo mv /tmp/config.toml /etc/drbd-ha/config.toml"
    echo "  → Config file created"
else
    echo "  → Config file already exists, skipping"
fi

# 3. Restart service if requested
if [[ "$RESTART" == true ]]; then
    echo "[3/3] Restarting service..."
    ssh "$REMOTE_HOST" "sudo systemctl restart drbd-ha"
    echo "  → Service restarted"
else
    echo "[3/3] Done! (service not restarted)"
fi

echo ""
echo "Deployed to: $REMOTE_HOST"
if [[ "$RESTART" == false ]]; then
    echo ""
    echo "To restart service manually:"
    echo "  ssh $REMOTE_HOST 'sudo systemctl restart drbd-ha'"
fi
