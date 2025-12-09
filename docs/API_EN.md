# DRBD HA Manager API Documentation

Base URL: `http://<host>:3373/api/v1`

## Authentication

If authentication is enabled in the configuration (`auth.enabled = true`), all API requests (except `/health`) require a Token in the Header:

```
Authorization: Bearer <your-token>
```

or

```
Authorization: Token <your-token>
```

Unauthenticated requests will return `401 Unauthorized`.

## Table of Contents

- [Health Check](#health-check)
- [Node Management](#node-management)
- [DRBD Resource Management](#drbd-resource-management)
- [HA Profile Management](#ha-profile-management)
- [drbd-reactor Management](#drbd-reactor-management)
- [Systemd Service Management](#systemd-service-management)
- [Real-time Event Stream (SSE)](#real-time-event-stream-sse)
- [Safety Checks](#safety-checks)

---

## Health Check

### GET /health

Check service health status.

**Response Example:**

```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

---

## Node Management

### GET /nodes

List all registered nodes.

**Response Example:**

```json
[
  {
    "id": "local",
    "hostname": "node1",
    "ip": "127.0.0.1",
    "ssh_port": 22,
    "ssh_user": "root",
    "is_local": true,
    "status": "online",
    "last_seen": "2024-01-15T10:30:00Z"
  },
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "hostname": "node2",
    "ip": "192.168.1.102",
    "ssh_port": 22,
    "ssh_user": "root",
    "is_local": false,
    "status": "online",
    "last_seen": "2024-01-15T10:30:00Z"
  }
]
```

### POST /nodes

Add a new node to the cluster.

**Request Body:**

```json
{
  "hostname": "node2",
  "ip": "192.168.1.102",
  "ssh_port": 22,
  "ssh_user": "root",
  "ssh_private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\n...",
  "ssh_key_path": "/root/.ssh/id_rsa",
  "ssh_password": null
}
```

| Field           | Type   | Required | Description                                    |
| --------------- | ------ | -------- | ---------------------------------------------- |
| hostname        | string | Yes      | Node hostname                                  |
| ip              | string | Yes      | Node IP address                                |
| ssh_port        | number | No       | SSH port, default 22                           |
| ssh_user        | string | No       | SSH username, default root                     |
| ssh_private_key | string | No       | SSH private key content (PEM format)           |
| ssh_key_path    | string | No       | SSH private key file path (e.g., "/root/.ssh/id_rsa") |
| ssh_password    | string | No       | SSH password (not recommended)                 |

**SSH Authentication Priority:** `ssh_private_key` > `ssh_key_path` > `ssh_password`

When using `ssh_key_path`, the system reads the private key from the specified path and caches it in memory. The key is automatically reloaded after service restart.

**Response:** `201 Created`

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hostname": "node2",
  "ip": "192.168.1.102",
  "ssh_port": 22,
  "ssh_user": "root",
  "is_local": false,
  "status": "online",
  "last_seen": "2024-01-15T10:30:00Z"
}
```

### GET /nodes/:id

Get information about a specific node.

**Response Example:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hostname": "node2",
  "ip": "192.168.1.102",
  "ssh_port": 22,
  "ssh_user": "root",
  "is_local": false,
  "status": "online",
  "last_seen": "2024-01-15T10:30:00Z"
}
```

### DELETE /nodes/:id

Remove a node from the cluster.

**Response:** `204 No Content`

### GET /nodes/:id/disks

List all block devices on a node.

**Response Example:**

```json
[
  {
    "name": "sda",
    "path": "/dev/sda",
    "size": 53687091200,
    "size_human": "50G",
    "type": "disk",
    "mountpoint": null,
    "fstype": null,
    "ro": false,
    "model": "VBOX HARDDISK",
    "children": [
      {
        "name": "sda1",
        "path": "/dev/sda1",
        "size": 52613349376,
        "type": "part",
        "mountpoint": "/",
        "fstype": "ext4"
      }
    ]
  },
  {
    "name": "sdb",
    "path": "/dev/sdb",
    "size": 10737418240,
    "size_human": "10G",
    "type": "disk",
    "mountpoint": null,
    "fstype": null,
    "ro": false,
    "model": "VBOX HARDDISK",
    "children": []
  }
]
```

### GET /nodes/:id/disks/available

List block devices available for DRBD (unmounted, no filesystem).

**Response Example:**

```json
[
  {
    "name": "sdb",
    "path": "/dev/sdb",
    "size": 10737418240,
    "size_human": "10G",
    "type": "disk",
    "mountpoint": null,
    "fstype": null,
    "ro": false,
    "model": "VBOX HARDDISK",
    "children": []
  }
]
```

### POST /nodes/:id/check

Check node connection status.

**Response Example:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hostname": "node2",
  "status": "online",
  "message": null
}
```

---

## DRBD Resource Management

### GET /resources

List all DRBD resources and their status.

**Response Example:**

```json
{
  "resources": [
    {
      "name": "r0",
      "role": "Primary",
      "devices": [
        {
          "volume": 0,
          "disk_state": "UpToDate",
          "minor": 0,
          "size": 10737418240
        }
      ],
      "connections": [
        {
          "peer_node_id": 1,
          "name": "node2",
          "connection_state": "Connected",
          "peer_devices": [
            {
              "volume": 0,
              "replication_state": "Established",
              "peer_disk_state": "UpToDate",
              "percent_in_sync": 100.0
            }
          ]
        }
      ]
    }
  ]
}
```

### POST /resources

Create a new DRBD resource.

**Request Body:**

```json
{
  "name": "r0",
  "port": 7789,
  "minor": 0,
  "node_disks": {
    "local": "/dev/sdb",
    "550e8400-e29b-41d4-a716-446655440000": "/dev/sdb"
  },
  "auto_promote": true
}
```

| Field        | Type    | Required | Description                                                              |
| ------------ | ------- | -------- | ------------------------------------------------------------------------ |
| name         | string  | Yes      | Resource name (starts with letter, max 64 chars)                         |
| port         | number  | Yes      | DRBD port (7000-8000)                                                    |
| minor        | number  | Yes      | DRBD minor number                                                        |
| node_disks   | object  | Yes      | Mapping of node ID to disk path                                          |
| auto_promote | boolean | No       | Enable auto-promote, default `true`. Set to `false` for drbd-reactor managed resources |

> **auto_promote Details:**
> - `true` (default): Standard DRBD resource, can be manually promoted to Primary on any node
> - `false`: For drbd-reactor/HA managed resources. Generated config will include:
>   - `auto-promote no` - Disable auto-promotion, controlled by drbd-reactor
>   - `on-suspended-primary-outdated force-secondary` - Force outdated primary to secondary
>   - `on-no-data-accessible io-error` - Return IO error when data is inaccessible
>
> These options are critical for HA scenarios to prevent split-brain and data inconsistency.

**Response:** `201 Created`

```json
{
  "name": "r0",
  "message": "Resource configuration created. Run 'up' action to initialize.",
  "config_path": "/etc/drbd.d/r0.res"
}
```

### GET /resources/:name

Get status of a specific resource.

**Response Example:**

```json
{
  "name": "r0",
  "role": "Primary",
  "devices": [...],
  "connections": [...]
}
```

### DELETE /resources/:name

Delete a DRBD resource.

**Response:** `204 No Content`

### POST /resources/:name/action

Perform an action on a resource.

**Request Body:**

```json
{
  "action": "primary",
  "force": false
}
```

| Action Value        | Description                                                |
| ------------------- | ---------------------------------------------------------- |
| up                  | Start the resource                                         |
| down                | Stop the resource                                          |
| primary             | Promote to primary                                         |
| secondary           | Demote to secondary                                        |
| connect             | Connect to peer                                            |
| disconnect          | Disconnect from peer                                       |
| invalidate          | Invalidate local data, trigger resync                      |
| verify              | Verify data consistency                                    |
| recover_split_brain | Recover from split brain (as victim, discard local data)   |

**Response Example:**

```json
{
  "resource": "r0",
  "action": "primary",
  "success": true,
  "message": null
}
```

### POST /resources/:name/init

Initialize resource (create metadata and start).

**Response Example:**

```json
{
  "resource": "r0",
  "action": "init",
  "success": true,
  "message": "Resource initialized and brought up"
}
```

### POST /resources/:name/mkfs

Create filesystem on DRBD device (resource must be Primary).

**Request Body:**

```json
{
  "fstype": "ext4",
  "force": false
}
```

| Field  | Type    | Required | Description                        |
| ------ | ------- | -------- | ---------------------------------- |
| fstype | string  | Yes      | Filesystem type: ext4, xfs, btrfs  |
| force  | boolean | No       | Force creation (dangerous)         |

**Response Example:**

```json
{
  "resource": "r0",
  "action": "mkfs.ext4",
  "success": true,
  "message": "Created ext4 filesystem on /dev/drbd0"
}
```

### POST /resources/:name/mount

Mount DRBD device (resource must be Primary).

**Request Body:**

```json
{
  "mount_point": "/mnt/data",
  "options": null
}
```

**Response Example:**

```json
{
  "resource": "r0",
  "action": "mount",
  "success": true,
  "message": "Mounted /dev/drbd0 at /mnt/data"
}
```

### POST /resources/:name/umount

Unmount DRBD device.

**Request Body:**

```json
{
  "mount_point": "/mnt/data"
}
```

**Response Example:**

```json
{
  "resource": "r0",
  "action": "umount",
  "success": true,
  "message": "Unmounted /mnt/data"
}
```

### GET /resources/:name/logs

Get DRBD resource related logs (from journalctl).

**Query Parameters:**

| Parameter | Type   | Description                                    |
| --------- | ------ | ---------------------------------------------- |
| lines     | number | Number of log lines to return, default 100, max 1000 |
| since     | string | Time filter (e.g., "1h", "30m", "2024-01-15")  |

**Response Example:**

```json
{
  "resource": "r0",
  "service": "drbd-promote@r0.service",
  "total_lines": 50,
  "lines": [
    "Jan 15 10:30:00 node1 systemd[1]: Starting DRBD promote service for r0...",
    "Jan 15 10:30:01 node1 drbd-promote[1234]: Resource r0 promoted to Primary"
  ]
}
```

---

## HA Profile Management

HA functionality is based on the **drbd-reactor promoter plugin**. When a DRBD resource becomes Primary, it automatically:

1. Mounts the DRBD device (via auto-generated systemd `.mount` unit)
2. Configures VIP (via `ocf:heartbeat:IPaddr2` resource agent)
3. Starts systemd services in order (with auto-generated service overrides)

When demoted to Secondary, it performs the reverse operations.

### Auto-Generated Systemd Units

When creating an HA profile, the system automatically generates:

1. **Mount Unit** (`/etc/systemd/system/<escaped-mount-point>.mount`)
   - Handles mounting DRBD device to the specified mount point
   - Depends on `drbd-promote@<resource>.service`
   - Example: `/var/lib/mysql` → `var-lib-mysql.mount`

2. **Service Overrides** (`/etc/systemd/system/<service>.d/ha-override.conf`)
   - Adds `BindsTo=` and `After=` dependencies on mount unit
   - Sets `DefaultDependencies=no` to prevent automatic startup
   - Ensures services stop when mount becomes unavailable

### GET /ha/profiles

List all HA profiles.

**Response Example:**

```json
{
  "profiles": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440001",
      "name": "mysql-ha",
      "resource_name": "r0",
      "mount_point": "/var/lib/mysql",
      "fs_type": "xfs",
      "vip": {
        "address": "192.168.1.100",
        "netmask": 24,
        "interface": "eth0"
      },
      "promoter": {
        "services": ["mysql.service"],
        "stop_on_demote": true,
        "on_demote_failure": "reboot"
      },
      "status": "active",
      "generated_units": {
        "mount_unit": "var-lib-mysql.mount",
        "mount_unit_path": "/etc/systemd/system/var-lib-mysql.mount",
        "drbd_device": "/dev/drbd/by-res/r0/0",
        "service_overrides": [
          {
            "service_name": "mysql.service",
            "override_dir": "/etc/systemd/system/mysql.service.d",
            "override_path": "/etc/systemd/system/mysql.service.d/ha-override.conf"
          }
        ]
      }
    }
  ]
}
```

### POST /ha/profiles

Create a new HA profile.

**Request Body:**

```json
{
  "name": "mysql-ha",
  "resource_name": "r0",
  "mount_point": "/var/lib/mysql",
  "fs_type": "xfs",
  "services": ["mysql.service"],
  "vip": {
    "address": "192.168.1.100",
    "netmask": 24,
    "interface": "eth0"
  },
  "stop_on_demote": true,
  "on_demote_failure": "reboot",
  "auto_disable_services": true,
  "migration": {
    "migrate_data": true,
    "source_path": "/var/lib/mysql",
    "format_device": true,
    "preserve_permissions": true
  }
}
```

| Field                       | Type    | Required | Description                                               |
| --------------------------- | ------- | -------- | --------------------------------------------------------- |
| name                        | string  | Yes      | Profile name                                              |
| resource_name               | string  | Yes      | Associated DRBD resource name                             |
| mount_point                 | string  | Yes      | DRBD device mount point                                   |
| fs_type                     | string  | No       | Filesystem type: xfs (default), ext4, btrfs               |
| services                    | array   | Yes      | List of services to start (in order)                      |
| vip                         | object  | No       | Virtual IP configuration                                  |
| vip.address                 | string  | Yes      | VIP address                                               |
| vip.netmask                 | number  | Yes      | Subnet mask (1-32)                                        |
| vip.interface               | string  | Yes      | Network interface                                         |
| stop_on_demote              | boolean | No       | Stop services on demote, default true                     |
| on_demote_failure           | string  | No       | Demote failure action: reboot/force/ignore                |
| auto_disable_services       | boolean | No       | Auto-disable managed services (systemctl disable), default true |
| migration                   | object  | No       | Data migration options                                    |
| migration.migrate_data      | boolean | No       | Whether to migrate existing data, default false           |
| migration.source_path       | string  | No       | Source directory for data migration (defaults to mount_point) |
| migration.format_device     | boolean | No       | Format DRBD device before migration, default true         |
| migration.preserve_permissions | boolean | No    | Preserve file permissions during migration, default true  |

> **Note**: `auto_disable_services` automatically disables services listed in `services` to prevent them from starting before DRBD is mounted on system reboot. This is the recommended behavior, as services should be started by drbd-reactor after DRBD becomes Primary.

> **Data Migration**: When `migration.migrate_data` is true, the system will:
> 1. Stop services listed in `services`
> 2. Promote DRBD resource to Primary
> 3. Format the device (if `format_device` is true)
> 4. Mount to a temporary directory
> 5. Use rsync to copy data from `source_path` to DRBD
> 6. Unmount and demote DRBD
> 7. Restart services

**Response:** `201 Created`

```json
{
  "profile": {
    "id": "550e8400-e29b-41d4-a716-446655440001",
    "name": "mysql-ha",
    "resource_name": "r0",
    "mount_point": "/var/lib/mysql",
    "fs_type": "xfs",
    "vip": {...},
    "promoter": {...},
    "status": "unknown",
    "generated_units": {
      "mount_unit": "var-lib-mysql.mount",
      "mount_unit_path": "/etc/systemd/system/var-lib-mysql.mount",
      "drbd_device": "/dev/drbd/by-res/r0/0",
      "service_overrides": [
        {
          "service_name": "mysql.service",
          "override_dir": "/etc/systemd/system/mysql.service.d",
          "override_path": "/etc/systemd/system/mysql.service.d/ha-override.conf"
        }
      ]
    }
  },
  "config_path": "/etc/drbd-reactor.d/mysql-ha.toml",
  "message": "Generated mount unit: var-lib-mysql.mount. Generated 1 service override(s). Generated promoter configuration. Disabled 1 service(s). Reload drbd-reactor to apply.",
  "disabled_services": ["mysql.service"],
  "generated_units": {
    "mount_unit": "var-lib-mysql.mount",
    "mount_unit_path": "/etc/systemd/system/var-lib-mysql.mount",
    "drbd_device": "/dev/drbd/by-res/r0/0",
    "service_overrides": [...]
  },
  "migration_result": {
    "bytes_transferred": 1234567890,
    "source_path": "/var/lib/mysql",
    "services_restarted": ["mysql.service"]
  }
}
```

**Generated systemd mount unit** (`/etc/systemd/system/var-lib-mysql.mount`):

```ini
# Auto-generated by drbd-ha
# DRBD Resource: r0
# DO NOT EDIT - Changes will be overwritten

[Unit]
Description=DRBD Mount for HA Profile (r0)
Documentation=man:systemd.mount(5)

# Wait for DRBD promote service to be active
After=drbd-promote@r0.service
BindsTo=drbd-promote@r0.service

# Ensure network is ready (for DRBD replication)
After=network-online.target
Wants=network-online.target

# Ordering with local-fs.target
Before=local-fs.target

[Mount]
What=/dev/drbd/by-res/r0/0
Where=/var/lib/mysql
Type=xfs
Options=defaults,noatime

[Install]
WantedBy=multi-user.target
```

**Generated service override** (`/etc/systemd/system/mysql.service.d/ha-override.conf`):

```ini
# Auto-generated by drbd-ha
# HA Profile: mysql-ha
# DO NOT EDIT - Changes will be overwritten
#
# This override ensures the service:
# 1. Waits for the DRBD mount to be available
# 2. Stops if the mount becomes unavailable
# 3. Does not start automatically on boot (managed by drbd-reactor)

[Unit]
# Service depends on mount - stops if mount is gone
BindsTo=var-lib-mysql.mount

# Service must start after mount is ready
After=var-lib-mysql.mount

# Also depend on network for services that need it
After=network-online.target

# Disable default dependencies to prevent automatic startup
# This service is managed by drbd-reactor, not by systemd boot process
DefaultDependencies=no

# Ensure proper shutdown ordering
Conflicts=shutdown.target
Before=shutdown.target
```

**Generated drbd-reactor config file** (`/etc/drbd-reactor.d/mysql-ha.toml`):

```toml
# drbd-reactor promoter configuration
# Generated by drbd-ha

[[promoter]]
[promoter.resources.r0]

[promoter.runner]
start = [
    "mysql.service",
]
stop-services-on-exit = true
on-drbd-demote-failure = "reboot"

[[promoter.runner.secondary]]
type = "ocf:heartbeat:IPaddr2"
name = "r0_vip"
[promoter.runner.secondary.attributes]
ip = "192.168.1.100"
cidr_netmask = "24"
nic = "eth0"
```

### GET /ha/profiles/:id

Get a specific HA profile.

### DELETE /ha/profiles/:id

Delete an HA profile. This also cleans up all generated systemd units:
- Removes service override files (`/etc/systemd/system/<service>.d/ha-override.conf`)
- Removes mount unit file (`/etc/systemd/system/<mount>.mount`)
- Removes promoter configuration file (`/etc/drbd-reactor.d/<name>.toml`)
- Reloads systemd daemon

**Response:** `204 No Content`

### GET /ha/profiles/:id/status

Get detailed status of an HA profile.

**Response Example:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440001",
  "name": "mysql-ha",
  "status": "active",
  "active_node": "gui01",
  "drbd": {
    "resource": "mysql-data",
    "role": "Primary",
    "disk": "UpToDate",
    "open": true,
    "peers": [
      {
        "name": "gui02",
        "role": "Secondary",
        "peer_disk": "UpToDate",
        "replication": "Established"
      }
    ]
  },
  "service_statuses": [
    {
      "name": "mysql.service",
      "active": true,
      "state": "active/running",
      "enabled": false
    }
  ],
  "vip_active": true,
  "config": {
    "promoter_config_exists": true,
    "promoter_config_path": "/etc/drbd-reactor.d/mysql-ha.toml",
    "reactor_running": true
  }
}
```

| Status Value | Description                      |
| ------------ | -------------------------------- |
| active       | Services running on this node    |
| standby      | Waiting to take over (Secondary) |
| stopped      | Services stopped                 |
| error        | Error state                      |
| unknown      | Unknown state                    |

**Active Node Field:**

| Field       | Type   | Description                                                                 |
| ----------- | ------ | --------------------------------------------------------------------------- |
| active_node | string | Hostname of the node currently running the HA services (obtained from drbd-reactorctl status). Returns null if detection fails. |

**DRBD Status Fields:**

| Field    | Type    | Description                                               |
| -------- | ------- | --------------------------------------------------------- |
| resource | string  | DRBD resource name                                        |
| role     | string  | Local node role (Primary/Secondary/Unknown)               |
| disk     | string  | Local disk state (UpToDate/Inconsistent/Diskless, etc.)   |
| open     | boolean | Whether the device is currently opened (mounted/in use)   |
| peers    | array   | Array of peer node status                                 |

**Peer Status Fields:**

| Field       | Type   | Description                                               |
| ----------- | ------ | --------------------------------------------------------- |
| name        | string | Peer node name                                            |
| role        | string | Peer node role (Primary/Secondary/Unknown)                |
| peer_disk   | string | Peer disk state (UpToDate/Inconsistent/Diskless, etc.)    |
| connection  | string | Connection state (optional, e.g., Connected/Connecting)   |
| replication | string | Replication state (optional, e.g., Established/SyncSource)|

**Service Status Fields:**

| Field   | Description                                              |
| ------- | -------------------------------------------------------- |
| name    | Service name                                             |
| active  | Whether the service is currently running                 |
| state   | Service state (active_state/sub_state)                   |
| enabled | Whether the service starts on boot (should be false for HA-managed services) |

**Configuration Visibility Fields:**

| Field                  | Description                              |
| ---------------------- | ---------------------------------------- |
| promoter_config_exists | Whether the promoter config file exists  |
| promoter_config_path   | Path to the promoter config file         |
| reactor_running        | Whether drbd-reactor service is running  |

### POST /ha/profiles/:id/activate

Manually activate HA profile (promote DRBD, mount, start services, add VIP).

**Response:** Returns updated status, same format as `GET /ha/profiles/:id/status`

### POST /ha/profiles/:id/deactivate

Manually deactivate HA profile (remove VIP, stop services, unmount, demote DRBD).

**Response:** Returns updated status

### POST /ha/profiles/:id/evict

Evict an HA profile from a specified node, triggering failover to another node. Uses `drbd-reactorctl evict` under the hood.

**Request Body:**

```json
{
  "node": "gui02",
  "delay": 30,
  "keep_masked": false,
  "force": false
}
```

| Field       | Type    | Required | Description                                                                 |
| ----------- | ------- | -------- | --------------------------------------------------------------------------- |
| node        | string  | No       | Target node hostname or ID to evict from (defaults to local node)           |
| delay       | number  | No       | Seconds to wait for peer takeover (default: 20)                             |
| keep_masked | boolean | No       | Keep target masked after eviction, prevents automatic failback (default: false) |
| force       | boolean | No       | Force eviction even with warnings (default: false)                          |

**Response Example:**

```json
{
  "success": true,
  "node": "gui02",
  "profile": "mongodb-ha",
  "message": "Successfully evicted mongodb-ha from node gui02. Another node should take over within 30 seconds.",
  "stdout": "...",
  "stderr": null
}
```

**Notes:**
- The evict command is executed on the specified node via SSH
- Another node in the cluster will automatically take over (which node depends on DRBD's promotion order)
- Use `keep_masked: true` to prevent the evicted node from taking over again automatically
- If you need to switch to a specific node, evict from all other nodes first with `keep_masked: true`

---

## drbd-reactor Management

### GET /ha/reactor/status

Get drbd-reactor service status.

**Response Example:**

```json
{
  "service": "drbd-reactor.service",
  "active_state": "active",
  "sub_state": "running",
  "running": true,
  "description": "DRBD Reactor Daemon"
}
```

### POST /ha/reactor/reload

Reload drbd-reactor configuration.

**Response Example:**

```json
{
  "success": true,
  "message": "drbd-reactor reloaded"
}
```

### GET /ha/reactor/logs

Get drbd-reactor service logs (from journalctl).

**Query Parameters:**

| Parameter | Type   | Description                                    |
| --------- | ------ | ---------------------------------------------- |
| lines     | number | Number of log lines to return, default 100, max 1000 |
| since     | string | Time filter (e.g., "1h", "30m", "2024-01-15")  |

**Response Example:**

```json
{
  "service": "drbd-reactor.service",
  "total_lines": 50,
  "lines": [
    "Jan 15 10:30:00 node1 drbd-reactor[1234]: Starting drbd-reactor...",
    "Jan 15 10:30:01 node1 drbd-reactor[1234]: Loaded promoter config for mysql-ha"
  ]
}
```

---

## Systemd Service Management

### GET /services

List currently running systemd services (for HA service selection). System services are filtered by default, showing only application services.

**Query Parameters:**

| Parameter      | Type    | Description                           |
| -------------- | ------- | ------------------------------------- |
| include_system | boolean | Include system services, default false|

**Response Example:**

```json
{
  "services": [
    {
      "name": "docker.service",
      "description": "Docker Application Container Engine",
      "load_state": "loaded",
      "active_state": "active",
      "sub_state": "running"
    },
    {
      "name": "nginx.service",
      "description": "A high performance web server",
      "load_state": "loaded",
      "active_state": "active",
      "sub_state": "running"
    },
    {
      "name": "postgresql.service",
      "description": "PostgreSQL RDBMS",
      "load_state": "loaded",
      "active_state": "inactive",
      "sub_state": "dead"
    }
  ]
}
```

### GET /services/available

List all available service unit files (including disabled services).

**Query Parameters:**

| Parameter      | Type    | Description                           |
| -------------- | ------- | ------------------------------------- |
| include_system | boolean | Include system services, default false|

**Response Example:**

```json
{
  "services": [
    {
      "name": "docker.service",
      "path": "/usr/lib/systemd/system/docker.service",
      "enabled_state": "enabled"
    },
    {
      "name": "mysql.service",
      "path": "/usr/lib/systemd/system/mysql.service",
      "enabled_state": "disabled"
    },
    {
      "name": "nginx.service",
      "path": "/usr/lib/systemd/system/nginx.service",
      "enabled_state": "enabled"
    }
  ]
}
```

---

## Real-time Event Stream (SSE)

DRBD HA Manager provides Server-Sent Events (SSE) interface for real-time frontend status updates.

### SSE Connection

```javascript
// JavaScript example
const eventSource = new EventSource('http://localhost:3373/api/v1/events/all');

eventSource.addEventListener('resource_status', (e) => {
  const data = JSON.parse(e.data);
  console.log('Resource status:', data);
});

eventSource.addEventListener('resource_change', (e) => {
  const data = JSON.parse(e.data);
  console.log('Resource changed:', data);
});

eventSource.addEventListener('node_status', (e) => {
  const data = JSON.parse(e.data);
  console.log('Node status:', data);
});

eventSource.addEventListener('progress', (e) => {
  const data = JSON.parse(e.data);
  console.log('Operation progress:', data);
});

eventSource.addEventListener('notification', (e) => {
  const data = JSON.parse(e.data);
  console.log('Notification:', data);
});

eventSource.addEventListener('heartbeat', (e) => {
  console.log('Heartbeat received');
});
```

### GET /events/all

Combined event stream containing all event types. Recommended for frontend use.

**Event Types:**

| Event Name        | Frequency    | Description                                |
| ----------------- | ------------ | ------------------------------------------ |
| resource_status   | Every 2 sec  | DRBD resource status update                |
| resource_change   | Real-time    | Resource state change (role, disk state)   |
| node_status       | Every 5 sec  | Node status update                         |
| progress          | Real-time    | Operation progress (from broadcast)        |
| notification      | Real-time    | System notification                        |
| heartbeat         | Every 30 sec | Keep-alive heartbeat                       |

**resource_status event data:**

```json
[
  {
    "name": "r0",
    "role": "Primary",
    "disk_state": "UpToDate",
    "connection_state": "Connected",
    "sync_percent": null
  }
]
```

**resource_change event data:**

```json
{
  "type": "resource_change",
  "data": {
    "name": "r0",
    "field": "role",
    "old_value": "Secondary",
    "new_value": "Primary",
    "timestamp": 1705312200
  }
}
```

**node_status event data:**

```json
[
  {
    "id": "local",
    "hostname": "node1",
    "status": "online",
    "last_seen": 1705312200
  },
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "hostname": "node2",
    "status": "online",
    "last_seen": 1705312195
  }
]
```

**progress event data:**

```json
{
  "operation_id": "op-123",
  "operation": "create_resource",
  "resource": "r0",
  "progress": 50,
  "message": "Writing config to node2...",
  "completed": false,
  "success": null
}
```

**notification event data:**

```json
{
  "level": "warning",
  "message": "Resource r0: Device has existing data",
  "source": "system",
  "timestamp": 1705312200
}
```

### GET /events/resources

DRBD resource status stream only.

- Sends `resource_status` event every 2 seconds
- Sends `resource_change` event on state changes

### GET /events/nodes

Node status stream only.

- Sends `node_status` event every 5 seconds

### GET /events/progress

Operation progress stream only.

- Sends `progress` events in real-time
- Sends `notification` events in real-time

### SSE Authentication

If Token authentication is enabled, SSE connections also require authentication:

```javascript
// Method 1: URL parameter (recommended for EventSource)
const eventSource = new EventSource(
  'http://localhost:3373/api/v1/events/all?token=your-token'
);

// Method 2: Using fetch + ReadableStream (supports Headers)
const response = await fetch('http://localhost:3373/api/v1/events/all', {
  headers: {
    'Authorization': 'Bearer your-token'
  }
});
const reader = response.body.getReader();
```

### Frontend Integration Example

```typescript
// React Hook example
function useDrbdEvents() {
  const [resources, setResources] = useState([]);
  const [nodes, setNodes] = useState([]);

  useEffect(() => {
    const es = new EventSource('/api/v1/events/all');

    es.addEventListener('resource_status', (e) => {
      setResources(JSON.parse(e.data));
    });

    es.addEventListener('node_status', (e) => {
      setNodes(JSON.parse(e.data));
    });

    es.addEventListener('resource_change', (e) => {
      const change = JSON.parse(e.data);
      // Show toast notification
      toast.info(`${change.name}: ${change.field} changed to ${change.new_value}`);
    });

    return () => es.close();
  }, []);

  return { resources, nodes };
}
```

---

## Safety Checks

DRBD HA Manager includes multiple layers of safety checks to prevent accidental operations that could lead to data loss or system damage.

### Disk Safety Checks

Disk safety checks are automatically performed before the following operations:

#### Create DRBD Resource (POST /resources)

- **System disk protection**: Automatically detects and refuses to use system disks (disks containing root filesystem)
- **Mount detection**: Checks if device is already mounted
- **Existing DRBD config detection**: Checks if device is already used by another DRBD resource
- **Existing data warning**: Logs a warning if device has existing data
- **Remote device checks**: Same safety checks are performed on all remote node devices

If checks fail, API returns `400 Bad Request`:

```json
{
  "error": "validation_error",
  "message": "Safety check failed for /dev/sda: Device /dev/sda appears to be the system disk. Refusing to use as DRBD backing device!"
}
```

#### Create Filesystem (POST /resources/:name/mkfs)

- **Device existence check**: Confirms device exists and is a block device
- **Mount detection**: Checks if device is already mounted
- **Existing filesystem detection**: Warns if device already has a filesystem
- **System disk protection**: Even for DRBD devices, checks if underlying device is system disk
- **Device usage detection**: Checks via `/sys/block/<device>/holders/` if device is used by LVM, MD, etc.

### Network Connectivity Checks

#### Network Verification Before Multi-node Operations (POST /resources)

Before creating DRBD resources involving multiple nodes, the system:

1. **Verifies all remote nodes are reachable**: Tests connectivity via SSH with simple commands
2. **Records response latency**: Records response time for each node
3. **All must pass to continue**: Operation is rejected if any node is unreachable

If network check fails, API returns `502 Bad Gateway`:

```json
{
  "error": "network_error",
  "message": "Cannot proceed: 1 node(s) unreachable: 192.168.1.102: Connection refused"
}
```

### Error Rollback Mechanism

#### Configuration File Write Rollback

When creating a DRBD resource, if writing configuration file to a remote node fails:

1. System automatically deletes configuration files written to other nodes
2. Also deletes local configuration file
3. Returns detailed error information

```
Write flow:
  local -> success ✓
  node2 -> success ✓
  node3 -> failed ✗

Rollback flow:
  node2 <- delete config
  local <- delete config
```

### Safety Check Summary

| Check Type         | Trigger Operation | Check Content                           | Failure Behavior |
| ------------------ | ----------------- | --------------------------------------- | ---------------- |
| Disk availability  | POST /resources   | Device exists, not mounted, not system  | Reject operation |
| DRBD reuse         | POST /resources   | Device not used by other DRBD resources | Reject operation |
| Network connectivity| POST /resources  | All remote nodes SSH reachable          | Reject operation |
| mkfs safety        | POST /mkfs        | Not mounted, not system, no holders     | Reject operation |
| Existing data warning| POST /resources | Device has existing data/filesystem     | Warning log      |
| Config rollback    | POST /resources   | Rollback written configs on failure     | Auto rollback    |

### Disabling Safety Checks

> **Warning**: Safety checks are designed to protect your data and system. Disabling them is strongly discouraged.

Currently, the API does not provide an option to disable safety checks. If you need to bypass checks in special circumstances, use the underlying `drbdadm` commands directly.

---

## Error Responses

All APIs return a unified error format:

```json
{
  "error": "validation_error",
  "message": "Invalid resource name 'bad;name'. Must start with a letter...",
  "details": null
}
```

| HTTP Status | Error Type        | Description                        |
| ----------- | ----------------- | ---------------------------------- |
| 400         | validation_error  | Input validation or safety check failed |
| 400         | json_error        | JSON parsing error                 |
| 404         | not_found         | Resource not found                 |
| 409         | already_exists    | Resource already exists            |
| 409         | conflict          | Operation conflict                 |
| 500         | drbd_error        | DRBD command execution error       |
| 500         | systemd_error     | Systemd operation error            |
| 500         | config_error      | Configuration file error           |
| 500         | database_error    | Database operation error           |
| 500         | transaction_error | Distributed transaction failed     |
| 502         | ssh_error         | SSH connection/execution error     |
| 502         | network_error     | Network connectivity check failed  |

---

## Complete Usage Examples

### 1. Initialize Cluster

```bash
# Add remote node
curl -X POST http://localhost:3373/api/v1/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "node2",
    "ip": "192.168.1.102",
    "ssh_private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\n..."
  }'

# Add third node (optional, supports multi-node)
curl -X POST http://localhost:3373/api/v1/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "node3",
    "ip": "192.168.1.103",
    "ssh_private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\n..."
  }'
```

### 2. Create DRBD Resource

```bash
# View available disks
curl http://localhost:3373/api/v1/nodes/local/disks/available

# Create 3-node DRBD resource (for HA, set auto_promote=false)
curl -X POST http://localhost:3373/api/v1/resources \
  -H "Content-Type: application/json" \
  -d '{
    "name": "r0",
    "port": 7789,
    "minor": 0,
    "node_disks": {
      "local": "/dev/sdb",
      "<node2-uuid>": "/dev/sdb",
      "<node3-uuid>": "/dev/sdb"
    },
    "auto_promote": false
  }'

# Initialize resource
curl -X POST http://localhost:3373/api/v1/resources/r0/init

# Promote to Primary (force required for first time)
curl -X POST http://localhost:3373/api/v1/resources/r0/action \
  -H "Content-Type: application/json" \
  -d '{"action": "primary", "force": true}'

# Create filesystem
curl -X POST http://localhost:3373/api/v1/resources/r0/mkfs \
  -H "Content-Type: application/json" \
  -d '{"fstype": "ext4"}'
```

### 3. Configure HA

```bash
# Create HA profile
curl -X POST http://localhost:3373/api/v1/ha/profiles \
  -H "Content-Type: application/json" \
  -d '{
    "name": "mysql-ha",
    "resource_name": "r0",
    "mount_point": "/var/lib/mysql",
    "services": ["mysql.service"],
    "vip": {
      "address": "192.168.1.100",
      "netmask": 24,
      "interface": "eth0"
    }
  }'

# Reload drbd-reactor
curl -X POST http://localhost:3373/api/v1/ha/reactor/reload

# Check status
curl http://localhost:3373/api/v1/ha/profiles/<profile-id>/status
```

### 4. Manual Failover

```bash
# Deactivate on current primary node
curl -X POST http://localhost:3373/api/v1/ha/profiles/<profile-id>/deactivate

# Activate on new primary node
curl -X POST http://localhost:3373/api/v1/ha/profiles/<profile-id>/activate
```
