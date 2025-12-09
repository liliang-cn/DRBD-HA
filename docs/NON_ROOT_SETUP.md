# 非 Root 用户配置指南

## 概述

DRBD HA Manager 支持使用非 root 用户（如 `ubuntu`）来管理远程节点。这需要额外的 sudo 配置。

## 场景

**典型环境**：

- 管理节点: `192.168.123.117` (gui01)
- 工作节点: `192.168.123.118` (gui02), `192.168.123.119` (gui03)
- SSH 用户: `ubuntu`（非 root）

## 配置步骤

### 1. 在每个工作节点上配置 Passwordless Sudo

在 **每个工作节点** (gui02, gui03) 上执行：

```bash
# SSH 到工作节点
ssh ubuntu@192.168.123.118

# 创建 sudoers 文件
sudo visudo -f /etc/sudoers.d/ubuntu

# 添加以下内容（保存并退出）
ubuntu ALL=(ALL) NOPASSWD:ALL

# 验证
sudo -n true && echo "OK" || echo "FAILED"

# 退出
exit
```

对 gui03 重复相同操作。

### 2. 在管理节点上运行 SSH 配置脚本

在 **管理节点** (gui01) 上：

```bash
# 切换到运行 drbd-ha 服务的用户（通常是 root）
sudo su -

# 运行 SSH 配置脚本
bash /tmp/setup-ssh.sh

# 输入提示信息：
# Enter the SSH username for remote nodes (default: root):
ubuntu

# Enter the IP addresses or hostnames of your cluster nodes (space separated):
192.168.123.118 192.168.123.119

# 然后为每个节点输入 ubuntu 用户的密码
```

脚本会自动：

- 生成 SSH 密钥（如果不存在）
- 复制公钥到 ubuntu@118 和 ubuntu@119
- 验证 SSH 免密登录
- **检查 sudo 权限**

期望输出：

```
=== Setup Complete ===

Verification Results:
--------------------
192.168.123.118 (ubuntu) ... ✓ SSH OK
  Sudo check ... ✓ Passwordless sudo
192.168.123.119 (ubuntu) ... ✓ SSH OK
  Sudo check ... ✓ Passwordless sudo
```

### 3. 验证配置

手动测试 SSH 和 sudo：

```bash
# 测试 SSH（不应该提示密码）
ssh -o BatchMode=yes ubuntu@192.168.123.118 "echo ok"
# 输出: ok

# 测试 sudo（不应该提示密码）
ssh -o BatchMode=yes ubuntu@192.168.123.118 "sudo -n lsblk"
# 输出: 磁盘列表
```

### 4. 更新 DRBD HA 配置（可选）

配置文件已经设置了 `default_user = "ubuntu"`，无需额外修改。

查看当前配置：

```bash
cat /etc/drbd-ha/config.toml | grep -A 2 "\[ssh\]"
```

应该看到：

```toml
[ssh]
# ...
default_user = "ubuntu"
```

## 使用

### 添加节点时

在 Web UI 中添加节点时：

- **Hostname**: gui02
- **IP**: 192.168.123.118
- **SSH Port**: 22
- **SSH User**: ubuntu (或留空使用默认值)

### 命令执行原理

当使用非 root 用户时，系统会自动为需要特权的命令添加 `sudo -n`：

```bash
# 原始命令
lsblk -J -b -o NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,RO,MODEL,PATH

# 实际执行（自动添加 sudo）
sudo -n lsblk -J -b -o NAME,SIZE,TYPE,MOUNTPOINT,FSTYPE,RO,MODEL,PATH
```

需要 sudo 的命令包括：

- LVM: `pvs`, `vgs`, `lvs`, `pvcreate`, `vgcreate`, `lvcreate` 等
- DRBD: `drbdadm`, `drbdsetup`, `drbdmeta`
- 系统: `systemctl`, `journalctl`, `mount`, `umount`
- 存储: `lsblk`, `mkfs`, `dd`
- 网络: `ip`, `iptables`
- iSCSI: `targetcli`
- NVMe-oF: `nvme`

## 故障排查

### 问题：节点显示 Unknown 状态

**原因**: SSH 或 sudo 配置不正确

**解决步骤**:

1. **测试 SSH 连接**

   ```bash
   ssh -o BatchMode=yes ubuntu@192.168.123.118 "echo ok"
   ```

   如果失败，重新运行 `setup-ssh.sh`

2. **测试 sudo 权限**

   ```bash
   ssh ubuntu@192.168.123.118 "sudo -n true"
   echo $?  # 应该输出 0
   ```

   如果失败，检查 `/etc/sudoers.d/ubuntu` 配置

3. **查看日志**
   ```bash
   sudo journalctl -u drbd-ha -f | grep -i "ssh\|sudo"
   ```

### 问题：sudo 提示需要密码

**症状**: 脚本显示 "⚠ Sudo may require password"

**解决方案**:

在工作节点上检查 sudoers 配置：

```bash
# 在工作节点上
sudo cat /etc/sudoers.d/ubuntu

# 应该包含：
ubuntu ALL=(ALL) NOPASSWD:ALL

# 检查文件权限
ls -l /etc/sudoers.d/ubuntu
# 应该是: -r--r----- 1 root root
```

如果文件不存在或内容不对：

```bash
sudo visudo -f /etc/sudoers.d/ubuntu
# 添加: ubuntu ALL=(ALL) NOPASSWD:ALL
```

### 问题：特定命令失败

**症状**: 某些操作报错 "Permission denied"

**调试**:

1. 查看实际执行的命令

   ```bash
   sudo journalctl -u drbd-ha -n 50 | grep "Executing"
   ```

2. 手动测试该命令

   ```bash
   ssh ubuntu@192.168.123.118 "sudo -n <command>"
   ```

3. 检查 sudo 日志（在工作节点上）
   ```bash
   sudo tail -f /var/log/auth.log | grep sudo
   ```

## 安全考虑

### 为什么需要 NOPASSWD

DRBD HA Manager 使用 `ssh -o BatchMode=yes`，这意味着：

- 不能提示输入密码
- 必须使用密钥认证
- sudo 也必须是 passwordless

### 限制 sudo 权限（可选）

如果不想给完全的 sudo 权限，可以限制到特定命令：

```bash
sudo visudo -f /etc/sudoers.d/ubuntu
```

添加：

```
# 限制到特定命令
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/lsblk
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/pvs
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/vgs
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/lvs
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/pvcreate
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/vgcreate
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/lvcreate
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/lvremove
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/vgremove
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/pvremove
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/drbdadm
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/drbdsetup
ubuntu ALL=(ALL) NOPASSWD: /usr/bin/systemctl
ubuntu ALL=(ALL) NOPASSWD: /usr/bin/journalctl
ubuntu ALL=(ALL) NOPASSWD: /usr/sbin/mkfs*
ubuntu ALL=(ALL) NOPASSWD: /usr/bin/mount
ubuntu ALL=(ALL) NOPASSWD: /usr/bin/umount
```

### SSH 密钥安全

1. **保护私钥**

   ```bash
   chmod 600 ~/.ssh/id_rsa
   chmod 700 ~/.ssh
   ```

2. **使用强密钥**

   ```bash
   ssh-keygen -t rsa -b 4096
   # 或更好：
   ssh-keygen -t ed25519
   ```

3. **限制 SSH 访问**（在工作节点上）
   ```bash
   # /etc/ssh/sshd_config
   PermitRootLogin no
   PasswordAuthentication no
   PubkeyAuthentication yes
   AllowUsers ubuntu
   ```

## 快速参考

### 完整配置命令（复制粘贴）

在管理节点 (gui01) 上：

```bash
# 1. 首先在每个工作节点配置 sudo
ssh ubuntu@192.168.123.118 "echo 'ubuntu ALL=(ALL) NOPASSWD:ALL' | sudo tee /etc/sudoers.d/ubuntu && sudo chmod 440 /etc/sudoers.d/ubuntu"
ssh ubuntu@192.168.123.119 "echo 'ubuntu ALL=(ALL) NOPASSWD:ALL' | sudo tee /etc/sudoers.d/ubuntu && sudo chmod 440 /etc/sudoers.d/ubuntu"

# 2. 运行 SSH 配置脚本
sudo bash /tmp/setup-ssh.sh
# 输入: ubuntu
# 输入: 192.168.123.118 192.168.123.119

# 3. 验证
ssh -o BatchMode=yes ubuntu@192.168.123.118 "sudo -n lsblk -J" && echo "✓ OK" || echo "✗ FAILED"
ssh -o BatchMode=yes ubuntu@192.168.123.119 "sudo -n lsblk -J" && echo "✓ OK" || echo "✗ FAILED"
```

### 检查清单

- [ ] 工作节点上有 ubuntu 用户
- [ ] ubuntu 用户配置了 passwordless sudo
- [ ] 管理节点可以免密 SSH 到 ubuntu@工作节点
- [ ] sudo 命令不提示密码 (`sudo -n` 成功)
- [ ] `/etc/drbd-ha/config.toml` 中 `default_user = "ubuntu"`
- [ ] 添加节点时使用 `ubuntu` 作为 SSH 用户
- [ ] 节点状态显示 `Online`

## 总结

使用非 root 用户的关键点：

1. ✅ **Passwordless SSH** - 使用 `setup-ssh.sh` 配置
2. ✅ **Passwordless Sudo** - 在工作节点配置 `/etc/sudoers.d/`
3. ✅ **正确的配置** - `default_user = "ubuntu"`
4. ✅ **自动 sudo 包装** - 代码自动为特权命令添加 `sudo -n`

这样就可以安全地使用非 root 用户管理整个 DRBD HA 集群！
