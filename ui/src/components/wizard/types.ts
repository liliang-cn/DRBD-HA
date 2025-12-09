import type { FormInstance } from "antd";
import type {
  HaType,
  BlockDevice,
  StoragePool,
  ServiceFileInfo,
} from "@/types";

export interface WizardSharedState {
  storageStrategy: "raw" | "lvm";
  setStorageStrategy: (strategy: "raw" | "lvm") => void;
  haType: HaType;
  setHaType: (type: HaType) => void;
  availableDisks: Record<string, BlockDevice[]>;
  setAvailableDisks: (disks: Record<string, BlockDevice[]>) => void;
  storagePools: StoragePool[];
  setStoragePools: (pools: StoragePool[]) => void;
  services: ServiceFileInfo[];
  setServices: (services: ServiceFileInfo[]) => void;
}

export interface StepProps {
  mode?: "service" | "storage";
  onNext?: () => void;
  onPrev?: () => void;
  loading?: boolean;
}
