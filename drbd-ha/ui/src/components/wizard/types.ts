import type {
  BlockDevice,
  HaType,
  Node,
  ServiceFileInfo,
  StoragePool,
} from '@/types';

export interface WizardSharedState {
  storageStrategy: 'raw' | 'lvm';
  setStorageStrategy: (strategy: 'raw' | 'lvm') => void;
  haType: HaType;
  setHaType: (type: HaType) => void;
  availableDisks: Record<string, BlockDevice[]>;
  setAvailableDisks: (disks: Record<string, BlockDevice[]>) => void;
  storagePools: StoragePool[];
  setStoragePools: (pools: StoragePool[]) => void;
  services: ServiceFileInfo[];
  setServices: (services: ServiceFileInfo[]) => void;
  selectedNodes: Node[];
  setSelectedNodes: (nodes: Node[]) => void;
  // Option to use existing DRBD resource and skip storage config
  useExistingResource: boolean;
  setUseExistingResource: (use: boolean) => void;
}

export interface StepProps {
  mode?: 'service' | 'storage';
  onNext?: () => void;
  onPrev?: () => void;
  loading?: boolean;
}
