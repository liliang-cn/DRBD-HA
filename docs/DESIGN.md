这是为您准备的 **HA-Forge 进阶功能详细设计文档**。

这份文档涵盖了 **LVM 存储池化**、**NFS 高可用** 以及 **存量数据迁移** 三大核心模块。这三个功能将使您的系统从一个简单的“数据库保姆”升级为真正的 **超融合基础设施（HCI）管理平台**。

---

# HA-Forge 进阶功能设计文档 (v2.0)

**版本:** 2.0
**模块:** LVM Storage, Data Migration, NFS HA
**目标:** 实现多租户存储隔离、无缝业务上云、文件共享高可用。

---

## 1\. 模块一：LVM 存储池管理 (Storage Pooling)

### 1.1 需求背景

目前系统是一块物理盘对应一个 DRBD 资源（Raw Disk 模式）。为了在一组物理节点上运行多个隔离的 HA 服务（如同时运行 MySQL, Redis, NFS），必须引入 LVM 将物理盘虚拟化为多个逻辑卷（Logical Volumes）。

### 1.2 技术要求

1.  **LVM2 工具集**: 节点必须安装 `lvm2`。
2.  **防止递归扫描 (Filter)**: **[关键]** 必须配置 LVM 过滤规则，禁止 LVM 扫描 DRBD 设备本身，防止出现 "Duplicate PV" 错误。
3.  **动态扩容**: 支持在线扩展 LV 和 DRBD 容量。

### 1.3 架构设计

引入 **存储池 (Storage Pool)** 和 **卷 (Volume)** 的概念。

```mermaid
graph TD
    Physical_Disk[/dev/sdb 1TB] --> |vgcreate| VG[LVM Volume Group (Pool)]
    VG --> |lvcreate| LV1[LV: mysql_data 50G]
    VG --> |lvcreate| LV2[LV: redis_data 10G]
    VG --> |lvcreate| LV3[LV: nfs_share 500G]

    LV1 --> DRBD1[Resource: res_mysql]
    LV2 --> DRBD2[Resource: res_redis]
    LV3 --> DRBD3[Resource: res_nfs]
```

### 1.4 Rust 核心实现

#### 数据模型 (`src/models/storage.rs`)

```rust
pub struct StoragePool {
    pub id: String,
    pub name: String,       // e.g., "ha_pool"
    pub device: String,     // e.g., "/dev/sdb"
    pub total_size: u64,
    pub free_size: u64,
}

pub struct Volume {
    pub name: String,       // e.g., "vol_mysql"
    pub pool_name: String,  // e.g., "ha_pool"
    pub size_gb: u64,
    pub device_path: String,// e.g., "/dev/ha_pool/vol_mysql"
}
```

#### 存储提供者 Trait (`src/core/storage/provider.rs`)

抽象化接口，以便未来支持 ZFS 或其他存储后端。

```rust
#[async_trait]
pub trait StorageProvider {
    /// 初始化存储池 (vgcreate)
    async fn init_pool(&self, disk: &str) -> Result<()>;

    /// 创建卷 (lvcreate)
    async fn create_volume(&self, vol_name: &str, size_gb: u64) -> Result<String>;

    /// 删除卷 (lvremove)
    async fn delete_volume(&self, vol_name: &str) -> Result<()>;

    /// 扩容 (lvextend)
    async fn resize_volume(&self, vol_name: &str, new_size_gb: u64) -> Result<()>;
}
```

### 1.5 LVM 安全配置 (至关重要)

在初始化 LVM Pool 时，Rust 后端必须检查并自动修正 `/etc/lvm/lvm.conf`，防止 LVM 识别 DRBD 设备。

**目标配置片段:**

```ini
devices {
    # 只扫描 sdX 和 nvme 设备，拒绝 drbd 设备
    filter = [ "a|/dev/sd.*|", "a|/dev/nvme.*|", "r|/dev/drbd.*|", "r|.*|" ]
    # 或者设置 global_filter
}
```

---

## 2\. 模块二：数据迁移引擎 (Data Migration Engine)

### 2.1 需求背景

用户拥有正在运行的单机服务（如 `/var/lib/mysql` 有 100GB 数据）。在启用 HA 时，需要将这些数据无损迁移到新建的 DRBD 卷中。

### 2.2 技术要求

1.  **Rsync**: 用于保持权限 (`-a`)、稀疏文件 (`-S`) 和硬链接 (`-H`) 的精确复制。
2.  **原子性**: 迁移失败必须能回滚，不能损坏原数据。
3.  **停机窗口**: 需要控制服务的启停。

### 2.3 流程设计

我们设计两种迁移策略：**冷迁移 (Standard)** 和 **最小停机迁移 (Pre-Copy)**。

#### 核心状态机 (Rust Workflow)

1.  **Pre-flight Check:**

    - 检查目标 DRBD 卷大小是否 \> 原目录占用空间。
    - 检查原服务是否由 Systemd 管理。

2.  **Pre-Copy (可选 - 热同步):**

    - _服务状态:_ **Running**
    - _动作:_ `rsync -av --delete /var/lib/mysql/ /mnt/drbd_tmp/`
    - _目的:_ 在业务不中断的情况下，搬运 90% 的静态旧数据。

3.  **Cutover (停机割接):**

    - _动作:_ `systemctl stop mysql`
    - _动作:_ `systemctl disable mysql`

4.  **Final Sync (最终同步):**

    - _动作:_ 再次执行 `rsync`。
    - _目的:_ 同步停机前最后一刻产生的差异数据（通常只需几秒到几分钟）。

5.  **Swap (切换):**

    - _动作:_ `mv /var/lib/mysql /var/lib/mysql.bak` (备份)
    - _动作:_ `mkdir /var/lib/mysql` && `chown mysql:mysql` (重建挂载点)

6.  **Takeover (接管):**

    - 生成 Reactor 配置并 Reload。

### 2.4 Rust 实现细节 (`src/core/migration.rs`)

```rust
pub async fn migrate(
    service: &str,
    src_dir: &str,
    drbd_dev: &str,
    pre_copy: bool
) -> Result<()> {
    let tmp_mount = "/mnt/.migration_tmp";
    mount_drbd(drbd_dev, tmp_mount).await?;

    // 1. 热同步阶段 (业务不中断)
    if pre_copy {
        info!("Starting Pre-Copy for {}...", service);
        run_rsync(src_dir, tmp_mount).await?;
    }

    // 2. 停机
    info!("Stopping service {}...", service);
    systemd::stop(service).await?;

    // 3. 最终同步
    info!("Starting Final Sync...");
    run_rsync(src_dir, tmp_mount).await?;

    // 4. 清理现场
    systemd::umount(tmp_mount).await?;
    backup_original_dir(src_dir).await?;
    recreate_mount_point(src_dir).await?;

    Ok(())
}

// rsync 封装 - 必须包含这些参数
fn run_rsync(src: &str, dest: &str) -> Result<()> {
    // -a: archive (权限, times, group, owner)
    // -H: hard links
    // -A: ACLs
    // -X: extended attributes
    // -S: sparse files (重要！针对数据库文件空洞)
    // --delete: 确保目标和源完全一致
    Command::new("rsync")
        .args(&["-avxHAXS", "--delete", src, dest])
        .status()
        .await?;
    Ok(())
}
```

---

## 3\. 模块三：NFS 高可用 (NFS HA)

### 3.1 需求背景

提供一个高可用的文件共享服务，当主节点宕机，VIP 和 NFS 服务自动漂移到备节点，客户端（Client）仅感知到短暂的 I/O 停顿，而不需要重新挂载。

### 3.2 架构设计

```
[Client] -> [VIP: 192.168.1.200] -> [NFS Server Process]
                                           |
                                      [Mount Point: /exports/share1]
                                           |
                                      [DRBD Resource: res_nfs]
                                           |
                                      [LVM Volume: vol_nfs (可选)]
```
- **核心**: 将 NFS 共享目录放在由 DRBD 保护的 LVM 逻辑卷上。
- **高可用**: 利用 `drbd-reactor` 确保当 DRBD 资源成为 Primary 时，NFS 服务、VIP 和挂载点能够自动迁移。

### 3.3 关键技术难点：NFS 状态同步

NFS Server 在 `/var/lib/nfs` 下存储了客户端的锁状态（Locks）和恢复信息。如果切换后这个目录是空的，客户端可能会遇到 `Stale file handle` 错误或锁丢失。

**解决方案**:
- **推荐方案**: 将 `/var/lib/nfs` 也放置在 DRBD 保护的存储上。在 `drbd-reactor` 启动 NFS 前，执行脚本将 `/var/lib/nfs` 软链接到 DRBD 挂载点下的隐藏目录。
- **简化方案**: 切换时重启 NFS Server 进程通常足够让客户端重试并恢复（NFSv3/v4 的 Grace Period），但可能丢失活跃锁。

### 3.4 配置文件生成与服务编排

当创建 NFS HA Profile 时，系统需要生成和管理以下配置：

#### A. Systemd Override (`nfs-server.service.d/ha-override.conf`)

NFS 服务需要绑定到我们生成的 Mount Unit，确保在挂载点就绪后启动。

```ini
[Unit]
BindsTo=exports-share1.mount
After=exports-share1.mount
```

#### B. Exports 配置 (`/etc/exports.d/ha_share1.exports`)

不要直接修改主 `/etc/exports`，而是生成独立文件以方便管理和删除。

```text
# 自动生成
/exports/share1  192.168.1.0/24(rw,sync,no_root_squash,no_subtree_check)
```

#### C. Reactor 配置 (`nfs_ha.toml`)

`drbd-reactor` 的 Promoter 配置将编排 NFS 服务的启动顺序：

```toml
[[promoter]]
[promoter.resources.res_nfs]
    start = [
        # 1. 挂载数据盘
        "exports-share1.mount",
        # 2. (可选) 处理 /var/lib/nfs 状态目录
        #    例如: "/usr/local/bin/ha-nfs-state-symlink.sh",
        # 3. 设置 VIP
        "ocf:heartbeat:IPaddr2 nfs_vip ip=192.168.1.200 cidr_netmask=24",
        # 4. 确保 exportfs 重新读取配置 (关键!)
        "ocf:heartbeat:exportfs name=nfs_exp fsid=1 directory=/exports/share1 clientspec='192.168.1.0/24' options='rw,sync,no_root_squash'",
        # 5. 启动 NFS Server
        "nfs-server.service"
    ]
```

_注意：这里推荐使用 `ocf:heartbeat:exportfs` 资源代理，它比直接写 `/etc/exports` 更稳健，因为它能在切换时动态加载/卸载导出目录。_

---

## 4\. 模块四：iSCSI 高可用 (iSCSI HA)

### 4.1 需求背景

为虚拟机、裸金属服务器或 Windows 集群提供高可用的块存储，通过 iSCSI 协议将 DRBD 保护的块设备暴露为网络 LUN (Logical Unit Number)。

### 4.2 架构设计

```
[iSCSI Initiator] -> [VIP: 192.168.1.201] -> [iSCSI Target Process (LIO)]
                                                   |
                                              [DRBD Resource: res_iscsi]
                                                   |
                                              [LVM Volume: vol_iscsi (可选)]
```
- **核心**: 将 DRBD 保护的 LVM 逻辑卷（或裸 DRBD 设备）直接通过 iSCSI Target 软件（如 Linux-IO Target, LIO）暴露。
- **高可用**: 利用 `drbd-reactor` 确保当 DRBD 资源成为 Primary 时，iSCSI Target 和 VIP 自动迁移。

### 4.3 技术要求

1.  **iSCSI Target 软件**: 节点必须安装 `targetcli` (或 `lio-utils`)。
2.  **LVM 卷或裸设备**: iSCSI Target 直接暴露块设备，不需要文件系统和挂载点。
3.  **Initiator 侧配置**: 客户端（如 VMware ESXi）需要配置 iSCSI Initiator。

### 4.4 配置文件生成与服务编排

当创建 iSCSI HA Profile 时，系统需要生成和管理以下配置：

#### A. TargetCLI 配置 (`targetcli` 命令)

主要通过执行 `targetcli` 命令来创建和管理 iSCSI Target。这包括：
- 创建 iSCSI Backstore (基于 `/dev/drbdX` 或 `/dev/vg_name/lv_name`)。
- 创建 iSCSI Target IQN。
- 创建 iSCSI Portal (绑定 VIP)。
- 创建 ACL (Access Control List) 限制访问。
- 导出 LUN。

**示例 `targetcli` 命令序列 (在 Primary 节点执行):**
```bash
# 创建 Backstore
targetcli /backstores/block create name=drbd_iscsi_lun0 dev=/dev/drbdX

# 创建 Target
targetcli /iscsi create iqn.2025-01.com.example:iscsi-storage

# 创建 Portal (绑定 VIP)
targetcli /iscsi/iqn.2025-01.com.example:iscsi-storage/tpg1/portals create 192.168.1.201

# 授权 Initiator 访问 (可选)
# targetcli /iscsi/iqn.../tpg1/acls create iqn.1991-05.com.microsoft:win-initiator

# 导出 LUN
targetcli /iscsi/iqn.../tpg1/luns create /backstores/block/drbd_iscsi_lun0

# 保存配置并退出
targetcli saveconfig
targetcli exit
```

#### B. Reactor 配置 (`iscsi_ha.toml`)

`drbd-reactor` 的 Promoter 配置将编排 iSCSI Target 的启动和 VIP 的管理。

```toml
[[promoter]]
[promoter.resources.res_iscsi]
    start = [
        # 1. 设置 VIP
        "ocf:heartbeat:IPaddr2 iscsi_vip ip=192.168.1.201 cidr_netmask=24",
        # 2. 启动 iSCSI Target 服务 (systemctl start target.service)
        "target.service",
        # 3. 确保 iSCSI Target 绑定到 DRBD 设备和 VIP
        #    这可能需要定制脚本或更高级的 OCF Agent 来动态加载 LIO 配置
        #    或在 target.service 启动后，通过 'targetcli' 命令动态修改配置
    ]
    # 降级时停止 target.service 并清理 targetcli 配置 (避免脑裂)
```

---

## 5\. 模块五：NVMe-oF 高可用 (NVMe-oF HA)

### 5.1 需求背景

为需要极高性能、低延迟块存储的应用（如 AI/ML、高性能数据库）提供高可用的 NVMe-oF Target。

### 5.2 架构设计

```
[NVMe Initiator] -> [VIP: 192.168.1.202] -> [NVMe-oF Target Process (kernel)]
                                                    |
                                               [DRBD Resource: res_nvmeof]
                                                    |
                                               [LVM Volume: vol_nvmeof (可选)]
```
- **核心**: 与 iSCSI 类似，DRBD 保护的 LVM 逻辑卷或裸设备作为 NVMe-oF 的后端存储。
- **高可用**: 利用 `drbd-reactor` 确保 NVMe-oF Target 和 VIP 自动迁移。

### 5.3 技术要求

1.  **内核模块**: `nvmet`, `nvmet_rdma` (如果使用 RoCE/InfiniBand) 或 `nvmet_tcp` (如果使用 TCP)。
2.  **NVMe-oF Target 配置**: 通常通过 `/sys` 文件系统或 `nvmetcli` 工具进行配置。
3.  ** Initiator 侧配置**: 客户端需要支持 NVMe-oF Initiator。

### 5.4 配置文件生成与服务编排

配置 NVMe-oF Target 比 iSCSI 更加底层和复杂，通常通过 `/sys` 伪文件系统进行。

**示例 `nvmetcli` 命令序列 (在 Primary 节点执行):**
```bash
# 假设 DRBD 设备是 /dev/drbdX

# 创建 Subsystem
nvmetcli create subsystem nqn.2025-01.com.example:nvme-storage

# 创建 Namespace (绑定到 DRBD 设备)
nvmetcli create namespace 1 on subsystem nqn.2025-01.com.example:nvme-storage
nvmetcli set subsystem nqn.2025-01.com.example:nvme-storage attr allow_any_host=1 # 简化设置
nvmetcli set subsystem nqn.2025-01.com.example:nvme-storage namespace 1 device path=/dev/drbdX

# 创建 Port (绑定 VIP 和 Fabric 类型)
nvmetcli create port 1
nvmetcli set port 1 addr adrfam=ipv4 traddr=192.168.1.202 trtype=tcp trsvcid=4420
nvmetcli set port 1 subsys nqn.2025-01.com.example:nvme-storage

# 启用 Port
nvmetcli set port 1 enable=1

# 保存配置
nvmetcli saveconfig
```

#### B. Reactor 配置 (`nvmeof_ha.toml`)

`drbd-reactor` 将编排 NVMe-oF Target 的启动和 VIP 的管理。

```toml
[[promoter]]
[promoter.resources.res_nvmeof]
    start = [
        # 1. 设置 VIP
        "ocf:heartbeat:IPaddr2 nvmeof_vip ip=192.168.1.202 cidr_netmask=24",
        # 2. 动态加载 NVMe-oF Target 配置
        #    这通常需要一个定制的 systemd service 或脚本来执行 nvmetcli 命令
        "ha-nvmeof-target.service" # 自定义服务来加载配置
    ]
    # 降级时清理配置
```

---

## 6\. API 设计概览 (更新)

为了支持上述功能，API 需要扩展：

### 6.1 存储池管理 (LVM)

- `POST /api/v1/pools` - 创建池 (LVM VG)
  - Body: `{ "name": "pool_ssd", "device": "/dev/sdb", "pool_type": "lvm" }`
- `GET /api/v1/pools` - 列出池及剩余空间

### 6.2 卷管理 (LVM LV)

- `POST /api/v1/pools/{pool_id}/volumes` - 创建卷 (LVM LV)
  - Body: `{ "name": "vol_mysql", "size_gb": 50 }`
  - _Response:_ 返回 `{ "id": "...", "name": "vol_mysql", "pool_id": "...", "size_gb": 50, "device_path": "/dev/pool_ssd/vol_mysql" }` (供 DRBD 使用)

### 6.3 NFS HA 创建

- `POST /api/v1/ha/nfs`
  - Body:
    ```json
    {
      "name": "office_share",
      "resource_name": "res_nfs",
      "mount_point": "/exports/share1",
      "fs_type": "xfs",
      "vip_address": "192.168.1.200",
      "vip_netmask": 24,
      "vip_interface": "eth0",
      "allowed_networks": ["192.168.1.0/24"],
      "options": "rw,sync,no_root_squash"
    }
    ```

### 6.4 iSCSI HA 创建

- `POST /api/v1/ha/iscsi`
  - Body:
    ```json
    {
      "name": "vmware_datastore",
      "resource_name": "res_iscsi",
      "vip_address": "192.168.1.201",
      "vip_netmask": 24,
      "vip_interface": "eth0",
      "iqn": "iqn.2025-01.com.haforge:vmware-lun1",
      "chap_user": "user",
      "chap_password": "password",
      "allowed_initiators": ["iqn.1991-05.com.microsoft:win-initiator"]
    }
    ```

### 6.5 NVMe-oF HA 创建

- `POST /api/v1/ha/nvmeof`
  - Body:
    ```json
    {
      "name": "ai_data",
      "resource_name": "res_nvmeof",
      "vip_address": "192.168.1.202",
      "vip_netmask": 24,
      "vip_interface": "eth0",
      "nqn": "nqn.2025-01.com.haforge:ai-subsys",
      "fabric_type": "tcp",
      "trsvcid": "4420",
      "allowed_nqns": ["nqn.2014-08.org.nvmexpress:uuid:5200921a-6379-33d8-aa8d-9669528659d9"]
    }
    ```

---

## 6.6 高级特性：多服务编排 (Service Orchestration)

在实际生产环境中，一个业务往往由多个服务组成，或者需要伴生服务（Sidecar）。例如：
- **LINSTOR Controller + FRPC**: 暴露内网 API 到公网。
- **Web App + Background Worker**: 主应用和后台任务。

HA-Forge 支持通过定义 `services` 列表的顺序来实现这种编排：

```json
"services": [
  "linstor-controller.service",
  "frpc@linstor.service"
]
```

**工作机制**:
1.  **启动顺序**: 当发生故障转移（Failover）或激活（Activate）时，drbd-reactor 会按照列表顺序依次启动服务 (`systemctl start`)。
    - 先启动 `linstor-controller`。
    - 待其启动成功后，再启动 `frpc@linstor`。
2.  **停止顺序**: 当需要降级（Demote）或停用时，drbd-reactor 会按照**相反顺序**依次停止服务。
    - 先停止 `frpc@linstor`。
    - 再停止 `linstor-controller`。
3.  **依赖管理**: 所有服务都会自动配置 `BindsTo` 依赖于底层的 DRBD 挂载点。如果挂载点失效，所有相关服务都会自动停止。

这种机制完美支持了 Sidecar 模式，确保附属服务（如代理、监控 agent）始终跟随主服务在同一个节点上运行。

---

## 7\. 模块七：仪表盘与全景监控 (Dashboard)

### 7.1 设计目标
为用户提供一站式的集群状态概览，一眼看清系统健康状况、资源拓扑和实时动态。

### 7.2 功能特性 (F01-F04)

#### [F01] 集群健康红绿灯
- **逻辑**: 聚合所有节点、DRBD 资源、HA 服务的状态。
- **状态定义**:
  - 🟢 **Healthy**: 所有节点在线，所有资源 UpToDate/Primary/Secondary，所有服务 Active。
  - 🟡 **Warning**: 节点离线但有 Quorum，DRBD 同步中，或服务处于 Standby。
  - 🔴 **Critical**: 脑裂 (Split Brain)，无 Quorum，资源 Diskless/Inconsistent 且无 Primary，或服务 Failed。
- **UI**: 顶部全局状态栏。

#### [F02] 实时拓扑图 (Topology Map)
- **可视化**: 使用 SVG 或 Canvas 绘制节点与资源的关系。
- **元素**:
  - 节点 (矩形): 显示 IP, Hostname。
  - 资源 (圆柱/连接线): 显示同步状态 (Syncing/Connected)。
  - 服务 (图标): 显示依附在哪个节点上。
- **动画**: 同步时连线闪烁或流动。

#### [F03] 关键指标卡片 (Metrics)
- **Cluster**: 在线节点数 / 总节点数。
- **Storage**: 已用容量 / 总容量 (LVM Pool 聚合)。
- **HA Profiles**: Active / Total Profiles。
- **Network**: 当前心跳延迟 (如果有监控)。

#### [F04] 实时事件流 (Live Log)
- **技术**: Server-Sent Events (SSE) `/api/v1/events/all`。
- **展示**: 侧边栏或底部滚动日志窗口。
- **内容**:
  - 操作日志: "User 'admin' created resource 'mysql-res'"
  - 系统事件: "Node 'node-2' disconnected"
  - 状态变更: "Resource 'r0' role changed: Secondary -> Primary"

### 7.3 API 支持
- 现有的 `/api/v1/events/all` 已支持 SSE。
- 需要扩展 `/api/v1/dashboard/summary` 接口以聚合红绿灯和拓扑数据。

---

## 8\. 实施路线图 (Implementation Roadmap) (更新)

1.  **Phase 1 (基础):** 实现 `LvmProvider`，重构 `create_resource` 逻辑支持 LVM 路径。
2.  **Phase 2 (工具):** 实现 `DataMigration` 模块，包含 Rsync 封装和回滚逻辑。
3.  **Phase 3 (服务):** 集成到 `ha/profiles` 接口，支持 MySQL/Redis 的“Import Data”选项。
4.  **Phase 4 (扩展 - 文件共享):** 实现 NFS HA 逻辑（集成 `exports` 配置，OCF `exportfs` agent）。
5.  **Phase 5 (扩展 - 块存储):** 实现 iSCSI HA 逻辑（集成 `targetcli` 配置，管理 `target.service`）。
6.  **Phase 6 (扩展 - 高性能块存储):** 实现 NVMe-oF HA 逻辑（集成 `nvmetcli` 配置，管理 `ha-nvmeof-target.service`）。
7.  **Phase 7 (UI):** 前端增加 Storage Pool 管理页面，HA Wizard 升级以支持 LVM、Data Migration，以及 NFS/iSCSI/NVMe-oF 创建向导。