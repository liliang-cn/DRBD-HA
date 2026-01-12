# DRBD-HA Internal Testing Release Checklist

## Release Package Preparation

### Automated Preparation (Recommended)

```bash
# Prepare release package (builds and packages automatically)
./scripts/prepare-release.sh

# Or specify version
./scripts/prepare-release.sh v0.1.0-beta
```

This creates:

- `drbd-ha-release-{version}/` - Release directory
- `drbd-ha-release-{version}.tar.gz` - Compressed archive

### Release Package Contents Verification

The packaged contents should include:

```
drbd-ha-release-{version}/
├── README.txt                    # Release notes
├── bin/
│   └── drbd-ha                   # Executable binary
├── config/
│   └── config.toml.example       # Configuration example
├── systemd/
│   └── drbd-ha.service           # Systemd service file
├── scripts/
│   ├── install.sh                # Installation script
│   └── uninstall.sh              # Uninstall script
└── docs/
    ├── QUICKSTART.md             # Quick start guide
    ├── DEPLOYMENT.md             # Deployment guide
    └── README.md                 # Project overview
```

## Testing Environment Requirements

### Minimum Test Environment

- **Number of Nodes**: 2-3 nodes
- **Operating System**: Ubuntu 20.04+ or RHEL 8+
- **Network**: Nodes must be able to communicate with each other
- **Permissions**: Root or sudo privileges

### Required Dependencies

All nodes must have the following installed:

```bash
# Ubuntu/Debian
apt-get install -y lvm2 drbd-utils drbd-dkms drbd-reactor

# RHEL/CentOS
yum install -y lvm2 drbd-utils kmod-drbd drbd-reactor
```

### Storage Requirements

Each node needs at minimum:

- One unused block device (for LVM/ZFS)
- Recommended: 20GB+ additional disk space

## Instructions for Testers

### 1. Files to Distribute

- `drbd-ha-release-{version}.tar.gz` - Main installation package

### 2. Quick Installation Guide

```bash
# 1. Extract
tar xzf drbd-ha-release-{version}.tar.gz
cd drbd-ha-release-{version}

# 2. Read documentation
cat README.txt
cat docs/QUICKSTART.md

# 3. Install
sudo ./scripts/install.sh

# 4. Configure (adjust for your environment)
sudo vim /etc/drbd-ha/config.toml

# 5. Start service
sudo systemctl start drbd-ha
sudo systemctl status drbd-ha

# 6. Access Web UI
# http://<server-ip>:3373
```

### 3. Testing Checklist

Suggested features for testers to verify:

#### Basic Functionality

- [ ] Web UI accessible
- [ ] Add nodes
- [ ] SSH connection test
- [ ] View node status

#### Storage Management

- [ ] Create LVM VG
- [ ] Create LVM LV
- [ ] Delete LV/VG

#### DRBD Resources

- [ ] Create DRBD resource
- [ ] Initialize metadata
- [ ] Start resource
- [ ] View status
- [ ] Delete resource

#### HA Configuration

- [ ] Create HA Profile (Simple mode)
- [ ] Create HA Profile (Advanced mode)
- [ ] View generated configuration
- [ ] Test failover
- [ ] Delete HA Profile

#### Advanced Features

- [ ] Evict operation
- [ ] Enable/disable configuration
- [ ] View logs

## Common Issues & Solutions

### SSH Connection Problems

```bash
# Generate key
ssh-keygen -t ed25519 -f ~/.ssh/id_drbd_ha -N ""

# Distribute to all nodes
for node in node1 node2 node3; do
    ssh-copy-id -i ~/.ssh/id_drbd_ha.pub root@$node
done

# Test connection
ssh -i ~/.ssh/id_drbd_ha root@node1 "hostname"
```

### Port Conflicts

```bash
# Check port usage
sudo lsof -i :3373

# Modify configuration
sudo vim /etc/drbd-ha/config.toml
# Change server.port

# Restart service
sudo systemctl restart drbd-ha
```

### View Logs

```bash
# Systemd logs
journalctl -u drbd-ha -f

# Application logs
tail -f /var/log/drbd-ha/drbd-ha.log
```

### DRBD Kernel Module Not Loaded

```bash
# Load module
sudo modprobe drbd

# Verify
lsmod | grep drbd
```

## Collecting Feedback

Suggested feedback information to collect:

1. **Environment Information**

   - OS version
   - Number of nodes
   - Network topology

2. **Functional Test Results**

   - Which features work correctly
   - Which features have issues
   - Error logs

3. **User Experience**

   - Is the UI intuitive?
   - Is the documentation clear?
   - Was installation smooth?

4. **Performance**
   - Response speed
   - Resource usage
   - Large-scale scenario performance

## Known Issues / Limitations

(Fill in based on actual situation)

- Currently only supports LVM and ZFS backends
- DRBD version requirement: 9.x
- Some error messages in UI may not be clear enough
- etc...

## Follow-up Support

Contact information for testers:

- Issue reporting channel
- Technical support contact
- Documentation update location
