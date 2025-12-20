---
# MySQL 远程访问 + 插入服务器 hostname 测试流程（完整示例）
---

## **1️⃣ 修改 MySQL 配置，监听所有地址**

编辑 MySQL 配置文件：

```bash
sudo vim /etc/mysql/mysql.conf.d/mysqld.cnf
```

在 `[mysqld]` 下添加或修改：

```ini
[mysqld]
bind-address = 0.0.0.0
```

- 这样 MySQL 会监听所有网卡（任何 host 可以访问）
- 保存文件并退出

重启 MySQL 服务：

```bash
sudo systemctl restart mysql
sudo systemctl status mysql
```

确认监听端口：

```bash
ss -lntp | grep 3306
```

- 正确输出应该是：

```
0.0.0.0:3306   0.0.0.0:*   LISTEN
```

---

## **2️⃣ 授权 root 用户远程访问**

登录 MySQL 本机：

```bash
sudo mysql
```

创建或修改 root 用户，使其可以从任意 host 远程访问并设置密码 `admin!`：

```sql
-- 创建或重置 root@% 用户
CREATE USER IF NOT EXISTS 'root'@'%' IDENTIFIED WITH mysql_native_password BY 'admin!';
GRANT ALL PRIVILEGES ON *.* TO 'root'@'%' WITH GRANT OPTION;
FLUSH PRIVILEGES;

-- 验证
SELECT user, host, plugin FROM mysql.user WHERE user='root';
```

> 结果中应显示 `root@%` 使用 `mysql_native_password`

---

## **3️⃣ 确保防火墙或安全组放行 3306**

Ubuntu/Debian (UFW)：

```bash
sudo ufw allow 3306/tcp
sudo ufw reload
```

CentOS / RHEL (firewalld)：

```bash
sudo firewall-cmd --add-port=3306/tcp --permanent
sudo firewall-cmd --reload
```

> 云服务器还需检查安全组规则是否允许 3306

---

## **4️⃣ 远程客户端连接**

在客户端机器上执行：

```bash
mysql -h 192.168.123.199 -P 3306 -u root -padmin!
```

- `192.168.123.199` → MySQL 服务器 IP
- `root` → 用户
- `admin!` → 密码

---

## **5️⃣ 创建测试数据库和表**

```sql
CREATE DATABASE IF NOT EXISTS ha_test;
USE ha_test;

CREATE TABLE IF NOT EXISTS messages (
    id INT AUTO_INCREMENT PRIMARY KEY,
    hostname VARCHAR(100),
    message VARCHAR(255),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

---

## **6️⃣ 插入一条服务器 hostname 消息**

```sql
INSERT INTO messages (hostname, message)
VALUES (@@hostname, 'This is a test message');
```

- `@@hostname` → MySQL 服务器主机名
- 这里不插入客户端信息

---

## **7️⃣ 查询验证**

```sql
SELECT * FROM messages;
```

示例输出：

| id  | hostname | message                | created_at          |
| --- | -------- | ---------------------- | ------------------- |
| 1   | orange2  | This is a test message | 2025-12-20 22:50:00 |

- `hostname` → MySQL 服务器名
- `message` → 测试消息

---

## **8️⃣ 完整流程总结**

1. 修改 MySQL 配置 `bind-address = 0.0.0.0`
2. 重启服务并验证监听端口
3. 创建或修改 `root@%` 用户并设置密码
4. 防火墙 / 安全组放行 3306
5. 客户端远程连接 MySQL
6. 创建数据库 `ha_test` 和表 `messages`
7. 插入一条消息，hostname 为服务器名
8. 查询验证

> 如果以上步骤全部成功，说明 **远程访问 + 写入测试完全可用**，适用于 HA / DRBD 场景。

---
