# DRBD HA Manager 用户故事

## 目标用户

系统管理员需要为关键服务（如数据库、文件服务器）配置高可用集群。

## 核心场景

### 1. 集群初始化

> 作为管理员，我想通过 API 添加集群节点，
> 这样我可以建立一个双节点 HA 集群。

**步骤：**

- `POST /api/v1/nodes` 添加 gui01 (192.168.123.117)
- `POST /api/v1/nodes` 添加 gui02 (192.168.123.118)
- 系统自动通过 SSH 验证节点连通性

### 2. 创建 DRBD 资源

> 作为管理员，我想创建一个 DRBD 复制资源，
> 这样两个节点之间的数据可以实时同步。

**步骤：**

- `POST /api/v1/resources` 指定磁盘设备、端口等
- 系统自动生成 `/etc/drbd.d/xxx.res` 配置
- 系统同步配置到所有节点
- 初始化并启动 DRBD 同步

### 3. 创建 HA Profile

> 作为管理员，我想为 MongoDB 创建 HA 配置，
> 这样服务可以在主节点故障时自动切换到备节点。

**步骤：**

- `POST /api/v1/ha/profiles` 指定：
  - DRBD 资源名
  - 挂载点 (`/var/lib/mongodb`)
  - 服务列表 (`mongod.service`)
  - VIP (可选)
- 系统自动生成：
  - systemd mount unit
  - service override (依赖 mount)
  - drbd-reactor promoter 配置

### 4. 日常监控

> 作为管理员，我想查看 HA 状态，
> 这样我能知道当前哪个节点在运行服务。

- `GET /api/v1/ha/profiles/:id/status`
- 返回：`active_node`、DRBD 状态、服务状态、VIP 状态

### 5. 计划内切换（维护）

> 作为管理员，我想把服务从 gui01 切换到 gui02，
> 这样我可以对 gui01 进行维护。

```bash
POST /api/v1/ha/profiles/:id/evict
{ "node": "gui01", "delay": 30 }
```

- 服务平滑迁移到 gui02

### 6. 故障自动切换

> 当 gui01 意外宕机时，
> drbd-reactor 自动检测到故障，
> 在 gui02 上：promote DRBD → mount → 启动服务 → 添加 VIP
> 整个过程无需人工干预。

## 架构简图

```
┌─────────────────┐         ┌─────────────────┐
│     gui01       │◄───────►│     gui02       │
│  (Primary)      │  DRBD   │  (Secondary)    │
│                 │  Sync   │                 │
│  ┌───────────┐  │         │  ┌───────────┐  │
│  │ MongoDB   │  │         │  │ (standby) │  │
│  │ VIP       │  │         │  │           │  │
│  └───────────┘  │         │  └───────────┘  │
│                 │         │                 │
│  drbd-reactor   │         │  drbd-reactor   │
└─────────────────┘         └─────────────────┘
         │
         │ REST API (:3373)
         ▼
┌─────────────────┐
│  drbd-ha│  ← 可部署在任一节点或管理节点
│  (本项目)        │
└─────────────────┘
```

## 价值主张

| 传统方式                | 使用本项目                      |
| ----------------------- | ------------------------------- |
| 手动编写 DRBD .res 文件 | API 自动生成并同步              |
| 手动配置 systemd 依赖   | 自动生成 mount unit 和 override |
| 手动配置 drbd-reactor   | 自动生成 promoter.toml          |
| 命令行切换              | REST API 一键切换               |
| 无统一状态视图          | API 返回完整集群状态            |
