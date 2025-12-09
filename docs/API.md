# DRBD HA Manager API 文档

基础 URL: `http://<host>:3373/api/v1`

## 认证

如果配置中启用了认证 (`auth.enabled = true`)，所有 API 请求（除 `/health` 外）需要在 Header 中携带 Token：

```
Authorization: Bearer <your-token>
```

或

```
Authorization: Token <your-token>
```

未认证的请求将返回 `401 Unauthorized`。

## 目录

- [健康检查](#健康检查)
- [节点管理](#节点管理)
- [DRBD 资源管理](#drbd-资源管理)
- [HA 配置管理](#ha-配置管理)
- [drbd-reactor 管理](#drbd-reactor-管理)
- [Systemd 服务管理](#systemd-服务管理)
- [实时事件流 (SSE)](#实时事件流-sse)
- [安全检查](#安全检查)

---

## 健康检查

### GET /health

检查服务健康状态。

**响应示例:**

```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

---

## 节点管理

### GET /nodes

列出所有已注册的节点。

**响应示例:**

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

添加新节点到集群。

**请求体:**

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

| 字段            | 类型   | 必填 | 说明                                           |
| --------------- | ------ | ---- | ---------------------------------------------- |
| hostname        | string | 是   | 节点主机名                                     |
| ip              | string | 是   | 节点 IP 地址                                   |
| ssh_port        | number | 否   | SSH 端口，默认 22                              |
| ssh_user        | string | 否   | SSH 用户名，默认 root                          |
| ssh_private_key | string | 否   | SSH 私钥内容 (PEM 格式)                        |
| ssh_key_path    | string | 否   | SSH 私钥文件路径 (如 "/root/.ssh/id_rsa")      |
| ssh_password    | string | 否   | SSH 密码 (不推荐)                              |

**SSH 认证优先级:** `ssh_private_key` > `ssh_key_path` > `ssh_password`

使用 `ssh_key_path` 时，系统会从指定路径读取私钥文件，并在内存中缓存。服务重启后会自动重新加载。

**响应:** `201 Created`

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

获取指定节点信息。

**响应示例:**

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

从集群中移除节点。

**响应:** `204 No Content`

### GET /nodes/:id/disks

列出节点上的所有块设备。

**响应示例:**

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

列出节点上可用于 DRBD 的块设备（未挂载、无文件系统）。

**响应示例:**

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

检查节点连接状态。

**响应示例:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "hostname": "node2",
  "status": "online",
  "message": null
}
```

---

## DRBD 资源管理

### GET /resources

列出所有 DRBD 资源及其状态。

**响应示例:**

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

创建新的 DRBD 资源。

**请求体:**

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

| 字段         | 类型    | 必填 | 说明                                                         |
| ------------ | ------- | ---- | ------------------------------------------------------------ |
| name         | string  | 是   | 资源名称 (字母开头，最长 64 字符)                            |
| port         | number  | 是   | DRBD 端口 (7000-8000)                                        |
| minor        | number  | 是   | DRBD minor 号                                                |
| node_disks   | object  | 是   | 节点 ID 到磁盘路径的映射                                     |
| auto_promote | boolean | 否   | 是否启用自动提升，默认 `true`。由 drbd-reactor 管理的资源应设为 `false` |

> **auto_promote 说明:**
> - `true` (默认): 普通 DRBD 资源，可在任何节点手动提升为 Primary
> - `false`: 用于 drbd-reactor/HA 管理的资源。生成的配置会包含：
>   - `auto-promote no` - 禁用自动提升，由 drbd-reactor 控制
>   - `on-suspended-primary-outdated force-secondary` - 过期的 primary 强制降为 secondary
>   - `on-no-data-accessible io-error` - 无法访问数据时返回 IO 错误
>
> 这些选项对于 HA 场景至关重要，可防止脑裂和数据不一致。

**响应:** `201 Created`

```json
{
  "name": "r0",
  "message": "Resource configuration created. Run 'up' action to initialize.",
  "config_path": "/etc/drbd.d/r0.res"
}
```

### GET /resources/:name

获取指定资源的状态。

**响应示例:**

```json
{
  "name": "r0",
  "role": "Primary",
  "devices": [...],
  "connections": [...]
}
```

### DELETE /resources/:name

删除 DRBD 资源。

**响应:** `204 No Content`

### POST /resources/:name/action

对资源执行操作。

**请求体:**

```json
{
  "action": "primary",
  "force": false
}
```

| action 值          | 说明                                               |
| ------------------ | -------------------------------------------------- |
| up                 | 启动资源                                           |
| down               | 停止资源                                           |
| primary            | 提升为主节点                                       |
| secondary          | 降级为从节点                                       |
| connect            | 连接到对端                                         |
| disconnect         | 断开对端连接                                       |
| invalidate         | 使本地数据失效，触发同步                           |
| verify             | 验证数据一致性                                     |
| recover_split_brain| 从脑裂状态恢复 (作为 victim，丢弃本地数据)         |

**响应示例:**

```json
{
  "resource": "r0",
  "action": "primary",
  "success": true,
  "message": null
}
```

### POST /resources/:name/init

初始化资源（创建元数据并启动）。

**响应示例:**

```json
{
  "resource": "r0",
  "action": "init",
  "success": true,
  "message": "Resource initialized and brought up"
}
```

### POST /resources/:name/mkfs

在 DRBD 设备上创建文件系统（资源必须是 Primary）。

**请求体:**

```json
{
  "fstype": "ext4",
  "force": false
}
```

| 字段   | 类型    | 必填 | 说明                           |
| ------ | ------- | ---- | ------------------------------ |
| fstype | string  | 是   | 文件系统类型: ext4, xfs, btrfs |
| force  | boolean | 否   | 强制创建 (危险)                |

**响应示例:**

```json
{
  "resource": "r0",
  "action": "mkfs.ext4",
  "success": true,
  "message": "Created ext4 filesystem on /dev/drbd0"
}
```

### POST /resources/:name/mount

挂载 DRBD 设备（资源必须是 Primary）。

**请求体:**

```json
{
  "mount_point": "/mnt/data",
  "options": null
}
```

**响应示例:**

```json
{
  "resource": "r0",
  "action": "mount",
  "success": true,
  "message": "Mounted /dev/drbd0 at /mnt/data"
}
```

### POST /resources/:name/umount

卸载 DRBD 设备。

**请求体:**

```json
{
  "mount_point": "/mnt/data"
}
```

**响应示例:**

```json
{
  "resource": "r0",
  "action": "umount",
  "success": true,
  "message": "Unmounted /mnt/data"
}
```

### GET /resources/:name/logs

获取 DRBD 资源相关的日志（来自 journalctl）。

**查询参数:**

| 参数  | 类型   | 说明                                        |
| ----- | ------ | ------------------------------------------- |
| lines | number | 返回的日志行数，默认 100，最大 1000         |
| since | string | 时间过滤 (如 "1h", "30m", "2024-01-15")     |

**响应示例:**

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

## Storage Pool Management

### GET /pools

List all storage pools (LVM Volume Groups).

**Response Example:**

```json
{
  "pools": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440002",
      "name": "vg_data",
      "device": "/dev/sdb",
      "total_size": 107374182400,
      "free_size": 53687091200
    }
  ]
}
```

### POST /pools

Create a new storage pool (LVM Volume Group).

**Request Body:**

```json
{
  "name": "vg_data",
  "device": "/dev/sdb",
  "pool_type": "lvm"
}
```

| Field     | Type   | Required | Description                                      |
| --------- | ------ | -------- | ------------------------------------------------ |
| name      | string | Yes      | Pool name (e.g., "vg_data")                      |
| device    | string | Yes      | Physical device path (e.g., "/dev/sdb")          |
| pool_type | string | Yes      | Pool type, currently only "lvm" is supported     |

**Response:** `201 Created`

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440002",
  "name": "vg_data",
  "device": "/dev/sdb",
  "total_size": 107374182400,
  "free_size": 107374182400
}
```

### POST /pools/:pool_id/volumes

Create a new logical volume in a storage pool.

**Request Body:**

```json
{
  "pool_id": "550e8400-e29b-41d4-a716-446655440002",
  "name": "vol_mysql",
  "size_gb": 50
}
```

| Field     | Type   | Required | Description                                      |
| --------- | ------ | -------- | ------------------------------------------------ |
| pool_id   | string | Yes      | ID of the storage pool                           |
| name      | string | Yes      | Volume name (e.g., "vol_mysql")                  |
| size_gb   | number | Yes      | Size in GB                                       |

**Response:** `201 Created`

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440003",
  "name": "vol_mysql",
  "pool_id": "550e8400-e29b-41d4-a716-446655440002",
  "size_gb": 50,
  "device_path": "/dev/vg_data/vol_mysql"
}
```

---

## HA 配置管理

HA 功能基于 **drbd-reactor promoter 插件** 实现，当 DRBD 资源变为 Primary 时自动：

1. 挂载 DRBD 设备（通过自动生成的 systemd `.mount` 单元）
2. 配置 VIP (通过 `ocf:heartbeat:IPaddr2` resource agent)
3. 按顺序启动 systemd 服务（带有自动生成的服务 override）

当降级为 Secondary 时自动执行相反操作。

### 自动生成的 Systemd 单元

创建 HA 配置时，系统会自动生成：

1. **Mount 单元** (`/etc/systemd/system/<转义后的挂载点>.mount`)
   - 处理 DRBD 设备到指定挂载点的挂载
   - 依赖 `drbd-promote@<resource>.service`
   - 示例: `/var/lib/mysql` → `var-lib-mysql.mount`

2. **服务 Override** (`/etc/systemd/system/<服务名>.d/ha-override.conf`)
   - 添加 `BindsTo=` 和 `After=` 依赖到 mount 单元
   - 设置 `DefaultDependencies=no` 防止自动启动
   - 确保当挂载不可用时服务会停止

### GET /ha/profiles

列出所有 HA 配置。

**响应示例:**

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

创建新的 HA 配置。

**请求体:**

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
  "lvm_pool_id": "550e8400-e29b-41d4-a716-446655440002",
  "lvm_volume_size_gb": 50,
  "migration": {
    "migrate_data": true,
    "source_path": "/var/lib/mysql",
    "format_device": true,
    "preserve_permissions": true
  }
}
```

| 字段                          | 类型    | 必填 | 说明                                                |
| ----------------------------- | ------- | ---- | --------------------------------------------------- |
| name                          | string  | 是   | 配置名称                                            |
| resource_name                 | string  | 是   | 关联的 DRBD 资源名                                  |
| mount_point                   | string  | 是   | DRBD 设备挂载点                                     |
| fs_type                       | string  | 否   | 文件系统类型: xfs (默认), ext4, btrfs               |
| services                      | array   | 是   | 要启动的服务列表 (按顺序)                           |
| vip                           | object  | 否   | 虚拟 IP 配置                                        |
| vip.address                   | string  | 是   | VIP 地址                                            |
| vip.netmask                   | number  | 是   | 子网掩码 (1-32)                                     |
| vip.interface                 | string  | 是   | 网络接口                                            |
| stop_on_demote                | boolean | 否   | 降级时停止服务，默认 true                           |
| on_demote_failure             | string  | 否   | 降级失败动作: reboot/force/ignore                   |
| auto_disable_services         | boolean | 否   | 自动禁用托管服务 (systemctl disable)，默认 true     |
| lvm_pool_id                   | string  | 否   | LVM 存储池 ID (若提供，将自动创建 LVM 卷)           |
| lvm_volume_size_gb            | number  | 否   | LVM 卷大小 (GB) (若提供 lvm_pool_id 则必填)         |
| migration                     | object  | 否   | 数据迁移选项                                        |
| migration.migrate_data        | boolean | 否   | 是否迁移现有数据，默认 false                        |
| migration.source_path         | string  | 否   | 数据迁移的源目录（默认为 mount_point）              |
| migration.format_device       | boolean | 否   | 迁移前是否格式化 DRBD 设备，默认 true               |
| migration.preserve_permissions| boolean | 否   | 迁移时是否保留文件权限，默认 true                   |

> **注意**: `auto_disable_services` 会自动禁用 `services` 中列出的服务，防止它们在系统重启时早于 DRBD 挂载而启动。这是推荐的做法，因为服务应该由 drbd-reactor 在 DRBD 成为 Primary 后自动启动。

> **数据迁移**: 当 `migration.migrate_data` 为 true 时，系统会：
> 1. 停止 `services` 中列出的服务
> 2. 将 DRBD 资源提升为 Primary
> 3. 格式化设备（如果 `format_device` 为 true）
> 4. 挂载到临时目录
> 5. 使用 rsync 从 `source_path` 复制数据到 DRBD
> 6. 卸载并将 DRBD 降级
> 7. 重启服务

**响应:** `201 Created`

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

**生成的 systemd mount 单元** (`/etc/systemd/system/var-lib-mysql.mount`):

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

**生成的服务 override** (`/etc/systemd/system/mysql.service.d/ha-override.conf`):

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

**生成的 drbd-reactor 配置文件** (`/etc/drbd-reactor.d/mysql-ha.toml`):

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

获取指定 HA 配置。

### DELETE /ha/profiles/:id

删除 HA 配置。同时会清理所有生成的 systemd 单元：
- 删除服务 override 文件 (`/etc/systemd/system/<service>.d/ha-override.conf`)
- 删除 mount 单元文件 (`/etc/systemd/system/<mount>.mount`)
- 删除 promoter 配置文件 (`/etc/drbd-reactor.d/<name>.toml`)
- 重新加载 systemd daemon

**响应:** `204 No Content`

### GET /ha/profiles/:id/status

获取 HA 配置的详细状态。

**响应示例:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440001",
  "name": "mysql-ha",
  "status": "active",
  "active_node": "node1",
  "drbd": {
    "resource": "r0",
    "role": "Primary",
    "disk": "UpToDate",
    "open": true,
    "peers": [
      {
        "name": "node2",
        "role": "Secondary",
        "peer_disk": "UpToDate"
      },
      {
        "name": "node3",
        "role": "Secondary",
        "peer_disk": "UpToDate"
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

| status 值 | 说明                 |
| --------- | -------------------- |
| active    | 服务正在本节点运行   |
| standby   | 等待接管 (Secondary) |
| stopped   | 服务已停止           |
| error     | 错误状态             |
| unknown   | 未知状态             |

**活跃节点字段说明:**

| 字段        | 说明                                                          |
| ----------- | ------------------------------------------------------------- |
| active_node | 当前活跃节点的主机名 (从 `drbd-reactorctl status` 获取)       |

**DRBD 状态字段说明:**

| 字段     | 说明                                              |
| -------- | ------------------------------------------------- |
| resource | DRBD 资源名称                                     |
| role     | 本地节点角色 (Primary/Secondary)                  |
| disk     | 本地磁盘状态 (UpToDate/Inconsistent/DUnknown 等)  |
| open     | 设备是否被打开/挂载                               |
| peers    | 对等节点状态列表                                  |

**对等节点状态字段:**

| 字段        | 说明                                              |
| ----------- | ------------------------------------------------- |
| name        | 对等节点主机名                                    |
| role        | 对等节点角色 (Primary/Secondary)                  |
| peer_disk   | 对等节点磁盘状态                                  |
| connection  | 连接状态 (Connected/Connecting 等，可选)          |
| replication | 复制状态 (Established/SyncSource 等，可选)        |

**服务状态字段说明:**

| 字段    | 说明                                                |
| ------- | --------------------------------------------------- |
| name    | 服务名称                                            |
| active  | 服务是否正在运行                                    |
| state   | 服务状态 (active_state/sub_state)                   |
| enabled | 服务是否开机启动 (HA 托管的服务通常应为 false)      |

**配置可见性字段说明:**

| 字段                   | 说明                                    |
| ---------------------- | --------------------------------------- |
| promoter_config_exists | promoter 配置文件是否存在               |
| promoter_config_path   | promoter 配置文件路径                   |
| reactor_running        | drbd-reactor 服务是否正在运行           |

### POST /ha/profiles/:id/activate

手动激活 HA 配置（提升 DRBD、挂载、启动服务、添加 VIP）。

**响应:** 返回更新后的状态，格式同 `GET /ha/profiles/:id/status`

### POST /ha/profiles/:id/deactivate

手动停用 HA 配置（移除 VIP、停止服务、卸载、降级 DRBD）。

**响应:** 返回更新后的状态

### POST /ha/profiles/:id/evict

从指定节点驱逐 HA 配置，触发故障转移到其他节点。底层使用 `drbd-reactorctl evict` 命令。

**请求体:**

```json
{
  "node": "gui02",
  "delay": 30,
  "keep_masked": false,
  "force": false
}
```

| 字段        | 类型    | 必填 | 描述                                                                |
| ----------- | ------- | ---- | ------------------------------------------------------------------- |
| node        | string  | 否   | 要驱逐的目标节点主机名或 ID（默认为本地节点）                       |
| delay       | number  | 否   | 等待其他节点接管的秒数（默认: 20）                                  |
| keep_masked | boolean | 否   | 驱逐后保持 target masked，防止自动回切（默认: false）               |
| force       | boolean | 否   | 强制驱逐，即使有警告（默认: false）                                 |

**响应示例:**

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

**说明:**
- evict 命令通过 SSH 在指定节点上执行
- 集群中的其他节点会自动接管（具体哪个节点取决于 DRBD 的提升顺序）
- 使用 `keep_masked: true` 防止被驱逐的节点自动回切
- 如果需要切换到特定节点，先用 `keep_masked: true` 驱逐其他所有节点

---

## drbd-reactor 管理

### GET /ha/reactor/status

获取 drbd-reactor 服务状态。

**响应示例:**

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

重新加载 drbd-reactor 配置。

**响应示例:**

```json
{
  "success": true,
  "message": "drbd-reactor reloaded"
}
```

### GET /ha/reactor/logs

获取 drbd-reactor 服务的日志（来自 journalctl）。

**查询参数:**

| 参数  | 类型   | 说明                                        |
| ----- | ------ | ------------------------------------------- |
| lines | number | 返回的日志行数，默认 100，最大 1000         |
| since | string | 时间过滤 (如 "1h", "30m", "2024-01-15")     |

**响应示例:**

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

## Systemd 服务管理

### GET /services

列出当前运行的 systemd 服务（用于 HA 服务选择）。默认过滤系统服务，只显示应用服务。

**查询参数:**

| 参数           | 类型    | 说明                         |
| -------------- | ------- | ---------------------------- |
| include_system | boolean | 是否包含系统服务，默认 false |

**响应示例:**

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

列出所有可用的服务 unit 文件（包括未启用的服务）。

**查询参数:**

| 参数           | 类型    | 说明                         |
| -------------- | ------- | ---------------------------- |
| include_system | boolean | 是否包含系统服务，默认 false |

**响应示例:**

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

## 实时事件流 (SSE)

DRBD HA Manager 提供 Server-Sent Events (SSE) 接口，用于前端实时更新状态。

### SSE 连接方式

```javascript
// JavaScript 示例
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

综合事件流，包含所有类型的事件。推荐前端使用此端点。

**事件类型:**

| 事件名            | 触发频率 | 说明                           |
| ----------------- | -------- | ------------------------------ |
| resource_status   | 每 2 秒  | DRBD 资源状态更新              |
| resource_change   | 实时     | 资源状态变化 (角色、磁盘状态等) |
| node_status       | 每 5 秒  | 节点状态更新                   |
| progress          | 实时     | 操作进度 (来自广播)            |
| notification      | 实时     | 系统通知                       |
| heartbeat         | 每 30 秒 | 心跳保活                       |

**resource_status 事件数据:**

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

**resource_change 事件数据:**

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

**node_status 事件数据:**

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

**progress 事件数据:**

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

**notification 事件数据:**

```json
{
  "level": "warning",
  "message": "Resource r0: Device has existing data",
  "source": "system",
  "timestamp": 1705312200
}
```

### GET /events/resources

仅 DRBD 资源状态流。

- 每 2 秒发送 `resource_status` 事件
- 状态变化时发送 `resource_change` 事件

### GET /events/nodes

仅节点状态流。

- 每 5 秒发送 `node_status` 事件

### GET /events/progress

仅操作进度流。

- 实时发送 `progress` 事件
- 实时发送 `notification` 事件

### SSE 认证

如果启用了 Token 认证，SSE 连接也需要认证。方式：

```javascript
// 方法 1: URL 参数 (推荐用于 EventSource)
const eventSource = new EventSource(
  'http://localhost:3373/api/v1/events/all?token=your-token'
);

// 方法 2: 使用 fetch + ReadableStream (支持 Header)
const response = await fetch('http://localhost:3373/api/v1/events/all', {
  headers: {
    'Authorization': 'Bearer your-token'
  }
});
const reader = response.body.getReader();
```

### 前端集成示例

```typescript
// React Hook 示例
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
      // 显示 toast 通知
      toast.info(`${change.name}: ${change.field} changed to ${change.new_value}`);
    });

    return () => es.close();
  }, []);

  return { resources, nodes };
}
```

---

## 安全检查

DRBD HA Manager 内置了多层安全检查机制，防止误操作导致数据丢失或系统损坏。

### 磁盘安全检查

在以下操作之前会自动执行磁盘安全检查：

#### 创建 DRBD 资源 (POST /resources)

- **系统盘保护**: 自动检测并拒绝使用系统盘（包含 root 文件系统的磁盘）
- **已挂载检测**: 检查设备是否已被挂载
- **已有 DRBD 配置检测**: 检查设备是否已被其他 DRBD 资源使用
- **现有数据警告**: 如果设备有现有数据，会在日志中记录警告
- **远程设备检查**: 对所有远程节点的设备也执行相同的安全检查

如果检查失败，API 返回 `400 Bad Request`：

```json
{
  "error": "validation_error",
  "message": "Safety check failed for /dev/sda: Device /dev/sda appears to be the system disk. Refusing to use as DRBD backing device!"
}
```

#### 创建文件系统 (POST /resources/:name/mkfs)

- **设备存在性检查**: 确认设备存在且为块设备
- **挂载检测**: 检查设备是否已被挂载
- **现有文件系统检测**: 如果设备已有文件系统，会发出警告
- **系统盘保护**: 即使是 DRBD 设备，也会检查底层设备是否为系统盘
- **设备使用检测**: 通过 `/sys/block/<device>/holders/` 检查设备是否被 LVM、MD 等使用

### 网络连通性检查

#### 多节点操作前的网络验证 (POST /resources)

在创建涉及多个节点的 DRBD 资源之前，系统会：

1. **验证所有远程节点可达**: 通过 SSH 执行简单命令测试连通性
2. **记录响应延迟**: 记录每个节点的响应时间
3. **全部通过才继续**: 如果任何节点不可达，操作会被拒绝

如果网络检查失败，API 返回 `502 Bad Gateway`：

```json
{
  "error": "network_error",
  "message": "Cannot proceed: 1 node(s) unreachable: 192.168.1.102: Connection refused"
}
```

### 错误回滚机制

#### 配置文件写入回滚

在创建 DRBD 资源时，如果向某个远程节点写入配置文件失败：

1. 系统会自动删除已写入到其他节点的配置文件
2. 同时删除本地的配置文件
3. 返回详细的错误信息

```
写入流程:
  本地 -> 成功 ✓
  node2 -> 成功 ✓
  node3 -> 失败 ✗

回滚流程:
  node2 <- 删除配置
  本地 <- 删除配置
```

### 安全检查类型汇总

| 检查类型     | 触发操作         | 检查内容                         | 失败行为 |
| ------------ | ---------------- | -------------------------------- | -------- |
| 磁盘可用性   | POST /resources  | 设备存在、未挂载、非系统盘       | 拒绝操作 |
| DRBD 复用    | POST /resources  | 设备未被其他 DRBD 资源使用       | 拒绝操作 |
| 网络连通性   | POST /resources  | 所有远程节点 SSH 可达            | 拒绝操作 |
| mkfs 安全    | POST /mkfs       | 设备未挂载、非系统盘、无 holders | 拒绝操作 |
| 现有数据警告 | POST /resources  | 设备有现有数据/文件系统          | 警告日志 |
| 配置回滚     | POST /resources  | 写入失败时回滚已写入的配置       | 自动回滚 |

### 禁用安全检查

> **警告**: 安全检查是为了保护您的数据和系统。强烈不建议禁用这些检查。

目前 API 不提供禁用安全检查的选项。如果您确实需要在特殊环境下绕过检查，请使用底层的 `drbdadm` 命令直接操作。

---

## 错误响应

所有 API 在出错时返回统一格式：

```json
{
  "error": "validation_error",
  "message": "Invalid resource name 'bad;name'. Must start with a letter...",
  "details": null
}
```

| HTTP 状态码 | error 类型        | 说明                 |
| ----------- | ----------------- | -------------------- |
| 400         | validation_error  | 输入验证或安全检查失败 |
| 400         | json_error        | JSON 解析错误        |
| 404         | not_found         | 资源不存在           |
| 409         | already_exists    | 资源已存在           |
| 409         | conflict          | 操作冲突             |
| 500         | drbd_error        | DRBD 命令执行错误    |
| 500         | systemd_error     | Systemd 操作错误     |
| 500         | config_error      | 配置文件错误         |
| 500         | database_error    | 数据库操作错误       |
| 500         | transaction_error | 分布式事务失败       |
| 502         | ssh_error         | SSH 连接/执行错误    |
| 502         | network_error     | 网络连通性检查失败   |

---

## 完整使用示例

### 1. 初始化集群

```bash
# 添加远程节点
curl -X POST http://localhost:3373/api/v1/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "node2",
    "ip": "192.168.1.102",
    "ssh_private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\n..."
  }'

# 添加第三个节点 (可选，支持多节点)
curl -X POST http://localhost:3373/api/v1/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "hostname": "node3",
    "ip": "192.168.1.103",
    "ssh_private_key": "-----BEGIN OPENSSH PRIVATE KEY-----\n..."
  }'
```

### 2. 创建 DRBD 资源

```bash
# 查看可用磁盘
curl http://localhost:3373/api/v1/nodes/local/disks/available

# 创建 3 节点 DRBD 资源 (用于 HA，设置 auto_promote=false)
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

# 初始化资源
curl -X POST http://localhost:3373/api/v1/resources/r0/init

# 提升为 Primary (首次需要 force)
curl -X POST http://localhost:3373/api/v1/resources/r0/action \
  -H "Content-Type: application/json" \
  -d '{"action": "primary", "force": true}'

# 创建文件系统
curl -X POST http://localhost:3373/api/v1/resources/r0/mkfs \
  -H "Content-Type: application/json" \
  -d '{"fstype": "ext4"}'
```

### 3. 配置 HA

```bash
# 创建 HA 配置
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

# 重新加载 drbd-reactor
curl -X POST http://localhost:3373/api/v1/ha/reactor/reload

# 查看状态
curl http://localhost:3373/api/v1/ha/profiles/<profile-id>/status
```

### 4. 手动故障转移

```bash
# 在当前主节点上停用
curl -X POST http://localhost:3373/api/v1/ha/profiles/<profile-id>/deactivate

# 在新主节点上激活
curl -X POST http://localhost:3373/api/v1/ha/profiles/<profile-id>/activate
```
