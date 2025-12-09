import { api } from "./client";
import type {
  StoragePool,
  CreateStoragePoolRequest,
  CreateStoragePoolResponse,
  CreateVolumeRequest,
  CreateVolumeResponse,
  Volume,
} from "@/types";

export const storageApi = {
  listPools: () => api.get<{ pools: StoragePool[] }>("/pools"),

  createPool: (data: { name: string; device?: string; pool_type: string; node_devices?: Record<string, string> }) =>
    api.post<{ id: string; name: string }>("/pools", data),

  createVolume: (poolId: string, data: CreateVolumeRequest) =>
    api.post<CreateVolumeResponse>(`/pools/${poolId}/volumes`, data),

  // Helper to list available volumes (might need backend support or filtering client-side)
  // For now we just assume we can list pools and the volumes are not directly listable globally yet
  // but usually queried per pool context if needed.
  // However, for the Wizard, we might want to pick a pool.
};
