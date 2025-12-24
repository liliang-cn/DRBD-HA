# DRBD HA Manager - Deployment Scripts

This directory contains helper scripts for deploying and managing the DRBD HA Manager service.

## Quick Start

```bash
# Deploy to remote server (builds locally, installs remotely)
./scripts/deploy.sh root@orange1

# Deploy to multiple servers
./scripts/deploy.sh root@orange1
./scripts/deploy.sh root@orange2
./scripts/deploy.sh root@orange3
```

## Scripts

### deploy.sh - Build and Deploy (Local Machine)

Builds the project **locally** and deploys to a **remote server**.

**Usage:**
```bash
./scripts/deploy.sh <user@host> [OPTIONS]
```

**Arguments:**
- `user@host` - Remote server (required)
  - Examples: `root@orange1`, `admin@192.168.1.100`

**Options:**
- `--skip-build` - Skip building, use existing binaries
- `--dev` - Build in debug mode instead of release

**Examples:**
```bash
# Build and deploy to orange1
./scripts/deploy.sh root@orange1

# Deploy pre-built binaries (skip build)
./scripts/deploy.sh root@orange1 --skip-build

# Debug build
./scripts/deploy.sh root@192.168.1.100 --dev

# Deploy to multiple servers
for host in orange1 orange2 orange3; do
    ./scripts/deploy.sh root@$host
done
```

**What it does:**
1. **Builds locally** (unless `--skip-build`)
   - Compiles Rust backend
   - Builds React UI
   - Embeds UI into binary

2. **Checks SSH connectivity** to remote server

3. **Transfers files via SCP**
   - Binary → `/tmp/drbd-ha-deploy/drbd-ha`
   - Config → `/tmp/drbd-ha-deploy/config.toml`
   - Install script → `/tmp/drbd-ha-deploy/install.sh`

4. **Executes install script** on remote server
   - Runs `sudo /tmp/drbd-ha-deploy/install.sh` via SSH
   - Cleans up temporary files

**Requirements (Local):**
- Makefile (run from project root)
- `ssh` and `scp` commands
- SSH access to remote server (password or key-based)

**Requirements (Remote):**
- Root or sudo privileges
- Linux with systemd
- `lvm2`, `drbd-utils`, `drbd-reactor` installed

---

### install.sh - Remote Installation (Remote Server)

Installs the DRBD HA Manager service **on the remote server**.

**Usage:**
```bash
sudo ./install.sh
```

**Note:** This script is typically executed automatically by `deploy.sh`. You would only run it manually if you have copied the files to the remote server yourself.

**What it does:**
1. Checks if running as root
2. Verifies system dependencies (lvm2, drbd-utils, drbd-reactor, systemd)
3. Creates directories (`/opt/drbd-ha`, `/etc/drbd-ha`, `/var/lib/drbd-ha`, `/var/log/drbd-ha`)
4. Installs binary from `/tmp/drbd-ha-deploy/drbd-ha` to `/opt/drbd-ha/drbd-ha`
5. Installs configuration from `/tmp/drbd-ha-deploy/config.toml` to `/etc/drbd-ha/config.toml`
6. Creates systemd service (`/etc/systemd/system/drbd-ha.service`)
7. Enables and starts the service

**Requirements:**
- Must run as root (via sudo)
- Binary and config must be in `/tmp/drbd-ha-deploy/`
- Linux with systemd

---

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

---

### update.sh - Remote Update (Local Machine)

Updates an **already-deployed** service on a remote server.

**Use case:**
- Update the binary without full reinstallation
- Quick iteration during development
- Apply bug fixes to production servers

**Usage:**
```bash
./scripts/update.sh <user@host> [OPTIONS]
```

**Arguments:**
- `user@host` - Remote server (required)
  - Examples: `root@orange1`, `admin@192.168.1.100`

**Options:**
- `--skip-build` - Skip building, update with existing binary
- `--dev` - Build in debug mode

**Examples:**
```bash
# Build and update
./scripts/update.sh root@orange1

# Update with existing binary
./scripts/update.sh root@orange1 --skip-build

# Debug build
./scripts/update.sh root@192.168.1.100 --dev

# Update multiple servers
for host in orange1 orange2 orange3; do
    ./scripts/update.sh root@$host
done
```

**What it does:**
1. **Builds locally** (unless `--skip-build`)
   - Compiles Rust backend
   - Builds React UI
   - Embeds UI into binary

2. **Stops the service** on remote server
   - `systemctl stop drbd-ha`

3. **Uploads new binary** via SCP
   - Binary → `/tmp/drbd-ha-update/drbd-ha`

4. **Replaces the old binary**
   - Copy to `/opt/drbd-ha/drbd-ha`
   - Set executable permissions

5. **Starts the service**
   - `systemctl start drbd-ha`

**What it preserves:**
- All configuration files (backs up config.toml before updating)
- All data in `/var/lib/drbd-ha/`
- All logs in `/var/log/drbd-ha/`
- systemd service configuration

**Requirements:**
- Service must already be installed (use `deploy.sh` for first-time installation)
- SSH access to remote server
- Build tools on local machine (for building)

**Comparison with deploy.sh:**

| Feature | deploy.sh | update.sh |
|---------|-----------|-----------|
| Purpose | First-time installation | Updating existing installation |
| Creates directories | Yes | No (already exists) |
| Creates systemd service | Yes | No (already exists) |
| Stops service | No | Yes (before update) |
| Backs up config | Yes | Yes |
| Overwrites binary | Yes | Yes |

---

## SSH Key Configuration

**IMPORTANT**: The drbd-ha service runs as **root**, so SSH keys must be configured for the root user.

### Why root?

The drbd-ha service needs to manage DRBD, LVM, and systemd services, which require root privileges. The service runs as root and uses SSH to execute commands on remote nodes. Therefore, SSH keys must be configured in `/root/.ssh/`, not in your regular user's home directory.

### Setup Instructions

On the machine where drbd-ha is running (as root):

```bash
# Switch to root shell
sudo -i

# Generate SSH key (if not exists)
ssh-keygen -t rsa -b 4096

# Copy public key to each remote node
ssh-copy-id liliang@orange2
ssh-copy-id liliang@orange3

# Test connection (should print "ok")
ssh -o BatchMode=yes liliang@orange2 echo ok
```

### Troubleshooting SSH Issues

If adding nodes in the UI fails with "Permission denied":

1. **Check service user:**
   ```bash
   ps aux | grep drbd-ha
   # Should show "root" as the user
   ```

2. **Check SSH keys location:**
   ```bash
   sudo ls -la /root/.ssh/
   # Should see id_rsa and id_rsa.pub
   ```

3. **Test SSH as root:**
   ```bash
   sudo ssh -o BatchMode=yes liliang@orange2 echo ok
   # Should print "ok"
   ```

4. **If your regular user has keys but root doesn't:**
   ```bash
   # Copy your keys to root
   sudo cp ~/.ssh/id_rsa* /root/.ssh/
   sudo cp ~/.ssh/known_hosts /root/.ssh/
   sudo chown -R root:root /root/.ssh/
   sudo chmod 600 /root/.ssh/id_rsa
   ```

## Deployment Workflow

The deployment process is split into two scripts for clear separation of concerns:

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Local Machine                                │
│  ┌─────────────┐     ┌──────────────┐     ┌──────────────────────┐  │
│  │ Makefile    │────▶│  cargo build │────▶│  target/release/     │  │
│  │  (UI+Backend)│     │   (Release)  │     │    drbd-ha           │  │
│  └─────────────┘     └──────────────┘     └──────────────────────┘  │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │  ./scripts/deploy.sh root@orange1                          │  │
│  │    │                                                       │  │
│  │    ├── Build project (make release)                        │  │
│  │    ├── Test SSH connection                                 │  │
│  │    └── SCP files ─────────────────────────────────┐       │  │
│  └────────────────────────────────────────────────────┼───────┘  │
└───────────────────────────────────────────────────────┼─────────┘
                                                        │
                                                        ▼ SCP
┌─────────────────────────────────────────────────────────────────┐
│                      Remote Server (orange1)                     │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  /tmp/drbd-ha-deploy/                                      │  │
│  │    ├── drbd-ha          (binary)                          │  │
│  │    ├── config.toml      (configuration)                   │  │
│  │    └── install.sh       (installation script)             │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  sudo ./install.sh                                        │  │
│  │    │                                                       │  │
│  │    ├── Create directories                                 │  │
│  │    ├── Copy binary → /opt/drbd-ha/drbd-ha                │  │
│  │    ├── Copy config → /etc/drbd-ha/config.toml            │  │
│  │    ├── Create systemd service                            │  │
│  │    └── Start service                                      │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Final Locations:                                          │  │
│  │    /opt/drbd-ha/drbd-ha       (binary)                    │  │
│  │    /etc/drbd-ha/config.toml   (configuration)             │  │
│  │    /etc/systemd/system/drbd-ha.service                   │  │
│  │    /var/lib/drbd-ha/          (data)                      │  │
│  │    /var/log/drbd-ha/          (logs)                      │  │
│  └───────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

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
   ./scripts/deploy.sh root@orange1
   ```

### SSH connection issues

If `deploy.sh` cannot connect to remote server:

1. Test SSH manually:
   ```bash
   ssh root@orange1
   ```

2. Setup SSH keys:
   ```bash
   ssh-copy-id root@orange1
   ```

3. If using password authentication, enter password when prompted

## Development Workflow

For development iterations:

```bash
# 1. Initial deployment (release mode)
./scripts/deploy.sh root@orange1

# 2. Make code changes
vim drbd-ha/src/...

# 3. Quick update (skip build, use existing binary)
./scripts/deploy.sh root@orange1 --skip-build

# 4. Or for development mode
./scripts/deploy.sh root@orange1 --dev
```

## Multi-Server Deployment

To deploy to multiple servers:

```bash
# Sequential deployment
for host in orange1 orange2 orange3; do
    ./scripts/deploy.sh root@$host
done

# Parallel deployment (using background jobs)
for host in orange1 orange2 orange3; do
    ./scripts/deploy.sh root@$host &
done
wait  # Wait for all deployments to complete
```

## Security Considerations

- **SSH Keys**: Setup passwordless SSH access for automated deployments. Keep private keys secure.
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
