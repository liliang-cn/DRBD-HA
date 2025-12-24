# DRBD HA Manager - Deployment Scripts

This directory contains helper scripts for deploying and managing the DRBD HA Manager service.

## Scripts

### deploy.sh - Automated Deployment

Deploys the DRBD HA Manager service with all necessary components.

**Usage:**
```bash
sudo ./scripts/deploy.sh [OPTIONS]
```

**Options:**
- `--skip-build` - Skip building, use existing binaries
- `--dev` - Build in debug mode instead of release

**What it does:**
1. Checks if running as root
2. Verifies system dependencies (lvm2, drbd-utils, drbd-reactor, systemd)
3. Builds UI and Backend (unless `--skip-build`)
4. Creates directories (`/opt/drbd-ha`, `/etc/drbd-ha`, `/var/lib/drbd-ha`, `/var/log/drbd-ha`)
5. Installs binary to `/opt/drbd-ha/drbd-ha`
6. Installs/updates configuration file
7. Creates systemd service (`/etc/systemd/system/drbd-ha.service`)
8. Enables and starts the service

**Examples:**
```bash
# Full deployment (build + install + start)
sudo ./scripts/deploy.sh

# Install pre-built binaries
sudo ./scripts/deploy.sh --skip-build

# Development mode (debug build)
sudo ./scripts/deploy.sh --dev
```

### uninstall.sh - Remove Service

Removes the DRBD HA Manager service from the system.

**Usage:**
```bash
sudo ./scripts/uninstall.sh [OPTIONS]
```

**Options:**
- `--purge-all` - Remove ALL configuration, data, and logs (not reversible)

**What it does:**
1. Stops and disables the service
2. Removes systemd service file
3. Removes binary from `/opt/drbd-ha/`
4. Optionally removes configuration, data, and logs (with `--purge-all`)

**What it does NOT remove:**
- DRBD resources
- LVM volumes or ZFS pools
- Running DRBD connections

**Examples:**
```bash
# Remove service but keep configuration/data
sudo ./scripts/uninstall.sh

# Remove everything including configuration/data
sudo ./scripts/uninstall.sh --purge-all
```

### setup-ssh.sh - SSH Key Setup

Configures passwordless SSH access from the manager node to cluster nodes.

**Usage:**
```bash
sudo ./scripts/setup-ssh.sh
```

**What it does:**
1. Generates SSH key pair if not exists
2. Copies public key to remote nodes
3. Verifies passwordless login
4. Tests sudo access (if not root)

**Setup process:**
- Prompts for remote SSH username (default: root)
- Prompts for list of node IPs/hostnames
- Copies SSH keys to each node
- Verifies passwordless access

## Installation Paths

After deployment, files are installed at:

| Component | Path |
|-----------|------|
| Binary | `/opt/drbd-ha/drbd-ha` |
| Configuration | `/etc/drbd-ha/config.toml` |
| Data | `/var/lib/drbd-ha/` |
| Logs | `/var/log/drbd-ha/drbd-ha.log` |
| Service | `/etc/systemd/system/drbd-ha.service` |

## Service Management

```bash
# Start service
sudo systemctl start drbd-ha

# Stop service
sudo systemctl stop drbd-ha

# Restart service
sudo systemctl restart drbd-ha

# Enable at boot
sudo systemctl enable drbd-ha

# Check status
sudo systemctl status drbd-ha

# View logs
sudo journalctl -u drbd-ha -f
# Or
sudo tail -f /var/log/drbd-ha/drbd-ha.log
```

## Troubleshooting

### Service won't start

1. Check if DRBD kernel module is loaded:
   ```bash
   lsmod | grep drbd
   sudo modprobe drbd
   ```

2. Check if drbd-reactor is running:
   ```bash
   sudo systemctl status drbd-reactor
   ```

3. Check service logs:
   ```bash
   sudo journalctl -u drbd-ha -n 50
   ```

4. Verify configuration:
   ```bash
   /opt/drbd-ha/drbd-ha --config /etc/drbd-ha/config.toml --check-config
   ```

### Build errors

If build fails:

1. Install Rust toolchain:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. Install Node.js for UI:
   ```bash
   # Ubuntu/Debian
   sudo apt-get install nodejs npm

   # RHEL/CentOS
   sudo yum install nodejs npm
   ```

3. Clean and rebuild:
   ```bash
   make clean
   sudo ./scripts/deploy.sh
   ```

### Permission denied errors

The scripts must be run as root because they:
- Install system files
- Create systemd services
- Start/stop services
- Access DRBD and LVM commands

Always use `sudo` when running deployment scripts.

## Development Workflow

For development iterations:

```bash
# 1. Initial deployment (release mode)
sudo ./scripts/deploy.sh

# 2. Make code changes
vim drbd-ha/src/...

# 3. Quick update (skip build, use existing binary)
sudo ./scripts/deploy.sh --skip-build

# 4. Or for development mode
sudo ./scripts/deploy.sh --dev
```

## Security Considerations

- **SSH Keys**: The `setup-ssh.sh` script configures passwordless SSH access. Keep private keys secure.
- **Root Access**: The service runs as root because it needs to manage DRBD, LVM, and systemd.
- **Firewall**: Port 3373 is exposed. Configure firewall rules as needed.
- **Configuration**: The config file may contain sensitive data. Set appropriate permissions:
  ```bash
  sudo chmod 600 /etc/drbd-ha/config.toml
  ```

## Related Documentation

- [Main README](../README.md) - Project overview and usage
- [API Documentation](../docs/API.md) - REST API reference
- [Configuration Guide](../docs/CONFIGURATION.md) - Detailed configuration options
