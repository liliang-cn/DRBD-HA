import { create } from 'zustand';
import type { DrbdResource, ResourceStatusEvent } from '@/types';
import { resourcesApi } from '@/api';

interface ResourcesState {
  resources: DrbdResource[];
  loading: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  updateFromSSE: (data: ResourceStatusEvent[]) => void;
}

export const useResourcesStore = create<ResourcesState>((set, get) => ({
  resources: [],
  loading: false,
  error: null,

  fetch: async () => {
    set({ loading: true, error: null });
    try {
      const { resources } = await resourcesApi.list();
      set({ resources, loading: false });
    } catch (err) {
      set({ error: (err as Error).message, loading: false });
    }
  },

  updateFromSSE: (data) => {
    const currentResources = get().resources;
    const updatedResources = currentResources.map((resource) => {
      const sseData = data.find((d) => d.name === resource.name);
      if (sseData) {
        return {
          ...resource,
          role: sseData.role,
          devices: resource.devices.map((device, index) =>
            index === 0
              ? { ...device, disk_state: sseData.disk_state }
              : device,
          ),
        };
      }
      return resource;
    });
    set({ resources: updatedResources });
  },
}));
