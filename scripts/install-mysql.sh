#!/bin/bash

# MySQL 安装脚本
# 在所有 HA 节点上并行安装 MySQL 服务器

# 节点列表
NODES=("orange1" "orange2" "orange3")

# 颜色输出
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}MySQL 并行安装脚本${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# 用于存储后台进程的 PID 数组
PIDS=()

# 遍历所有节点，启动并行安装
for node in "${NODES[@]}"; do
    echo -e "${BLUE}启动安装进程: $node${NC}"

    # 在后台执行 SSH 安装
    (
        echo -e "${YELLOW}[$node] 开始安装...${NC}"
        ssh "$node" "sudo apt install -y mysql-server" 2>&1 | while IFS= read -r line; do
            echo -e "${YELLOW}[$node]${NC} $line"
        done

        if [ ${PIPESTATUS[0]} -eq 0 ]; then
            echo -e "${GREEN}[$node] ✓ 安装成功${NC}"
        else
            echo -e "${RED}[$node] ✗ 安装失败${NC}"
        fi
    ) &

    # 记录后台进程 PID
    PIDS+=($!)
done

echo ""
echo -e "${BLUE}所有安装进程已启动，等待完成...${NC}"
echo ""

# 等待所有后台进程完成
for pid in "${PIDS[@]}"; do
    wait $pid
done

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}所有安装任务完成！${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "你可以使用以下命令检查 MySQL 服务状态："
for node in "${NODES[@]}"; do
    echo "  ssh $node 'sudo systemctl status mysql'"
done
