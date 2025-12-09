# SSH 配置指南

## 概述

DRBD HA Manager 使用 **系统 SSH** 来管理远程集群节点，不存储密码或密钥文件。所有 SSH 连接使用 `BatchMode=yes`，这意味着必须配置免密登录。

## 工作原理

### 添加节点时的权限检测

当你通过 Web UI 或 API 添加新节点时：

1. **输入验证** - 检查 hostname、IP 是否有效
2. **重复检测** - 确保 IP/hostname 不重复
3. **SSH 连接测试**（非阻塞）：
   ```bash
   ssh -p <port> \
       -o BatchMode=yes \
       -o StrictHostKeyChecking=no \
       -o UserKnownHostsFile=/dev/null \
       -o ConnectTimeout=5 \
       <user>@<ip> "echo ok"
   ```
4. **状态判断**：
   - 成功 → `Online`
   - 失败 → `Unknown`
   - **不会阻止节点添加**

### SSH 命令参数说明

- `BatchMode=yes` - 禁止交互式密码提示，必须使用密钥认证
- `StrictHostKeyChecking=no` - 自动接受新主机密钥
- `UserKnownHostsFile=/dev/null` - 不保存主机密钥
- `ConnectTimeout=5` - 5 秒连接超时

## 配置步骤

### 方法一：使用自动化脚本（推荐）

在 **管理节点**（运行 drbd-ha 服务的机器）上：

```bash
# 1. 切换到 root 用户（或 drbd-ha 服务运行的用户）
sudo su -

# 2. 运行 SSH 配置脚本
cd /path/to/drbd-ha-manager
bash scripts/setup-ssh.sh

# 3. 按提示输入节点 IP（空格分隔）
# 示例输入: 192.168.123.118 192.168.123.119

# 4. 为每个节点输入 root 密码
```

脚本会自动：

- 生成 SSH 密钥对（如果不存在）
- 将公钥复制到所有节点
- 验证免密登录

### 方法二：手动配置

```bash
# 1. 生成 SSH 密钥（如果不存在）
ssh-keygen -t rsa -b 4096 -f ~/.ssh/id_rsa -N ""

# 2. 复制公钥到每个节点
ssh-copy-id root@192.168.123.118
ssh-copy-id root@192.168.123.119

# 3. 验证免密登录
ssh -o BatchMode=yes root@192.168.123.118 "echo ok"
ssh -o BatchMode=yes root@192.168.123.119 "echo ok"
```

## 验证配置

### 测试免密 SSH

```bash
# 从管理节点测试
ssh -o BatchMode=yes -o ConnectTimeout=5 root@<node-ip> "echo ok"

# 期望输出: ok
# 退出码: 0
```

如果失败：

```bash
# 输出: Permission denied (publickey,password).
# 退出码: 255
```

### 检查节点状态

通过 API 检查：

```bash
curl http://<manager-ip>:3373/api/v1/nodes | jq '.[] | {id, hostname, ip, status}'
```

期望状态：

- `Online` - SSH 配置正确 ✅
- `Unknown` - SSH 未配置或失败 ❌
- `Offline` - 节点不可达 ❌

## 常见问题

### Q: 为什么不能使用密码登录？

A: DRBD HA Manager 设计为使用 SSH 密钥认证，原因：

- **安全性** - 不需要在应用中存储密码
- **自动化** - 支持无人值守操作
- **标准实践** - 符合 SSH 最佳实践

### Q: 节点显示 Unknown 状态怎么办？

A: 按以下步骤排查：

1. **检查网络连通性**

   ```bash
   ping <node-ip>
   ```

2. **测试 SSH 连接**

   ```bash
   ssh root@<node-ip>
   ```

   如果能手动登录但需要密码，说明需要配置密钥。

3. **验证 BatchMode SSH**

   ```bash
   ssh -o BatchMode=yes root@<node-ip> "echo ok"
   ```

   必须成功且不提示密码。

4. **重新配置 SSH**
   ```bash
   ssh-copy-id root@<node-ip>
   ```

### Q: 可以使用非 root 用户吗？

A: 可以，但该用户需要：

- 无密码 sudo 权限（用于 LVM、DRBD 操作）
- 配置好 SSH 密钥

修改配置文件 `/etc/drbd-ha/config.toml`：

```toml
[ssh]
default_user = "ubuntu"  # 或其他用户
```

### Q: 多个管理节点如何配置？

A: 每个管理节点都需要独立配置到工作节点的 SSH 访问：

```bash
# 在管理节点 A
ssh-copy-id root@worker1
ssh-copy-id root@worker2

# 在管理节点 B
ssh-copy-id root@worker1
ssh-copy-id root@worker2
```

## 安全建议

1. **限制 SSH 访问**
   在工作节点的 `/etc/ssh/sshd_config` 中：

   ```
   PermitRootLogin prohibit-password
   PubkeyAuthentication yes
   PasswordAuthentication no
   ```

2. **使用防火墙**
   只允许管理节点 IP 访问 SSH：

   ```bash
   ufw allow from <manager-ip> to any port 22
   ufw deny 22
   ```

3. **密钥权限**
   确保密钥文件权限正确：
   ```bash
   chmod 700 ~/.ssh
   chmod 600 ~/.ssh/id_rsa
   chmod 644 ~/.ssh/id_rsa.pub
   ```

## 故障排除日志

查看 DRBD HA 服务日志中的 SSH 相关错误：

```bash
sudo journalctl -u drbd-ha -f | grep -i ssh
```

典型错误信息：

- `Permission denied` - 未配置密钥或密钥不匹配
- `Connection timeout` - 网络不通或防火墙阻止
- `Host key verification failed` - 主机密钥变更（已自动忽略）

## 总结

| 场景                    | SSH 配置状态 | 节点状态  | 功能                      |
| ----------------------- | ------------ | --------- | ------------------------- |
| 已运行 setup-ssh.sh     | ✅ 已配置    | `Online`  | ✅ 可以管理远程磁盘、资源 |
| 未配置 SSH              | ❌ 未配置    | `Unknown` | ❌ 无法访问远程节点       |
| 手动 SSH 可用（需密码） | ⚠️ 部分配置  | `Unknown` | ❌ BatchMode 失败         |

**建议**：在添加任何远程节点之前，先运行 `scripts/setup-ssh.sh` 完成 SSH 配置。
