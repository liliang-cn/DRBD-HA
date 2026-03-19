#!/bin/bash
set -e

# Multi-Host Deployment Script (Parallel)
# Usage: ./scripts/deploy-all.sh <user@host1> <user@host2> ... [--restart]
#        ./scripts/deploy-all.sh host1,host2,host3 [--restart]

if [[ $# -eq 0 ]]; then
    echo "Usage: $0 <user@host1> <user@host2> ... [--restart]"
    echo "        $0 host1,host2,host3 [--restart]"
    echo "Example: $0 ubuntu@orange1 ubuntu@orange2 ubuntu@orange3"
    echo "         $0 orange1,orange2,orange3 --restart"
    echo ""
    echo "Options:"
    echo "  --restart       Restart active drbd-ha services after deployment"
    echo "  --skip-build    Skip building (use existing binary)"
    echo ""
    echo "Note: Supports both space-separated and comma-separated host lists"
    exit 1
fi

REMOTE_HOSTS=()
RESTART_FLAG=""
SKIP_BUILD=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --restart)
            RESTART_FLAG="--restart"
            shift
            ;;
        --skip-build)
            SKIP_BUILD="--skip-build"
            shift
            ;;
        *)
            # Check if argument contains comma (comma-separated list)
            if [[ "$1" == *,* ]]; then
                # Split by comma
                IFS=',' read -ra HOSTS <<< "$1"
                REMOTE_HOSTS+=("${HOSTS[@]}")
            else
                REMOTE_HOSTS+=("$1")
            fi
            shift
            ;;
    esac
done

if [[ ${#REMOTE_HOSTS[@]} -eq 0 ]]; then
    echo "Error: No hosts specified"
    exit 1
fi

echo "=== Deploying to ${#REMOTE_HOSTS[@]} hosts (parallel) ==="

# 1. Build once (unless --skip-build)
if [[ -z "$SKIP_BUILD" ]]; then
    echo "[1/3] Building..."
    make release
else
    echo "[1/3] Skipping build (using existing binary)..."
fi

# 2. Deploy to all hosts in parallel
echo "[2/3] Deploying (parallel)..."
for host in "${REMOTE_HOSTS[@]}"; do
    (
        echo "  → Deploying to $host..."
        ./scripts/deploy.sh "$host" --skip-build $RESTART_FLAG
        echo "  ✓ $host done"
    ) &
done

# Wait for all deployments to complete
wait

# 3. Done
echo "[3/3] Done!"
echo ""
echo "Deployed to: ${REMOTE_HOSTS[*]}"

if [[ -z "$RESTART_FLAG" ]]; then
    echo ""
    echo "To restart services on all hosts manually:"
    for host in "${REMOTE_HOSTS[@]}"; do
        echo "  ssh $host 'sudo systemctl restart drbd-ha' &"
    done
    echo "wait"
fi
