#!/bin/bash
set -e

# Multi-Host Deployment Script (Parallel)
# Usage: ./scripts/deploy-all.sh <user@host1> <user@host2> ...

if [[ $# -eq 0 ]]; then
    echo "Usage: $0 <user@host1> <user@host2> ..."
    echo "Example: $0 root@node1 root@node2 root@node3"
    exit 1
fi

REMOTE_HOSTS=("$@")

echo "=== Deploying to ${#REMOTE_HOSTS[@]} hosts (parallel) ==="

# 1. Build once
echo "[1/3] Building..."
make release

# 2. Deploy to all hosts in parallel
echo "[2/3] Deploying (parallel)..."
for host in "${REMOTE_HOSTS[@]}"; do
    (
        echo "  → Deploying to $host..."
        ./scripts/deploy.sh "$host" --skip-build
        echo "  ✓ $host done"
    ) &
done

# Wait for all deployments to complete
wait

# 3. Done
echo "[3/3] Done!"
echo ""
echo "Deployed to: ${REMOTE_HOSTS[*]}"
echo ""
echo "To restart services on all hosts:"
for host in "${REMOTE_HOSTS[@]}"; do
    echo "  ssh $host 'sudo systemctl restart drbd-ha' &"
done
echo "wait"
