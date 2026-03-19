---
name: deploy test
description: Deploy DRBD-HA to test environment and run basic tests
---

## deploy test

使用 ./scipts/deploy-all.sh orange1,orange2,orange3 命令部署DRBD-HA到测试环境，然后运行基本测试以验证部署是否成功。

hosts: orange1,orange2,orange3, 本机已经配置好免密登录到orange1,orange2,orange3和ssh config

部署后，使用 install-mysql.sh 脚本安装MySQL 和 intall-postgresql.sh 脚本安装PostgreSQL。
