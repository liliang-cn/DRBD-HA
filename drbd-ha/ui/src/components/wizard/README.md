# Wizard 组件

向导组件已被重构为多个小组件，提高了代码可维护性和复用性。

## 文件结构

```
ui/src/
├── pages/
│   └── Wizard.tsx (558 行) - 主向导逻辑和状态管理
└── components/
    └── wizard/
        ├── index.ts - 导出所有组件
        ├── types.ts - 共享类型定义
        ├── NodesVerificationStep.tsx - 步骤1: 节点验证
        ├── StorageConfigStep.tsx - 步骤2: 存储配置
        ├── HaConfigStep.tsx - 步骤3: HA配置
        └── ActivationStep.tsx - 步骤4: 激活状态
```

## 重构前后对比

- **重构前**: 单一文件 1117 行
- **重构后**: 主文件 558 行 + 4 个步骤组件 (~150 行/个)
- **改进**: 代码行数减少 50%，组件职责更清晰

## 组件说明

### NodesVerificationStep

显示集群节点列表并验证节点数量。

**Props:**

- `nodes: Node[]` - 集群节点列表

### StorageConfigStep

配置存储策略（Raw Disk 或 LVM Pool）。

**Props:**

- `form: WizardFormInstance` - 向导表单实例（见 `@/lib/wizard-form`）
- `storageStrategy: "raw" | "lvm"` - 当前选择的存储策略
- `onStrategyChange: (strategy) => void` - 策略变更回调
- `nodes: Node[]` - 节点列表
- `availableDisks: Record<string, BlockDevice[]>` - 可用磁盘
- `storagePools: StoragePool[]` - 存储池列表

### HaConfigStep

配置 HA 类型和协议特定参数（NFS/iSCSI/NVMe-oF/Generic）。

**Props:**

- `form: FormInstance` - 表单实例
- `mode: "service" | "storage"` - 向导模式
- `haType: HaType` - HA 类型
- `onHaTypeChange: (type) => void` - 类型变更回调
- `storageStrategy: "raw" | "lvm"` - 存储策略
- `resources: Resource[]` - DRBD 资源列表
- `services: ServiceFileInfo[]` - 可用服务列表

### ActivationStep

显示激活进度、成功或失败状态。

**Props:**

- `activationStatus: "pending" | "creating" | "activating" | "checking" | "success" | "error"`
- `activationError: string | null` - 错误信息
- `progressPercent: number` - 进度百分比
- `progressSteps: Array<{message, done}>` - 进度步骤
- `onRetry?: () => void` - 重试回调
- `onDone?: () => void` - 完成回调

## 使用示例

```tsx
import { Wizard } from "@/pages/Wizard";

// 服务 HA 向导
<Wizard mode="service" />

// 存储共享向导
<Wizard mode="storage" />
```

## 维护建议

1. **单一职责**: 每个步骤组件只负责一个向导步骤的 UI 渲染
2. **状态提升**: 所有状态管理在 Wizard.tsx 中，组件保持纯粹
3. **类型安全**: 使用 TypeScript 类型确保 props 正确传递
4. **独立测试**: 各步骤组件可独立测试，无需完整向导流程
