import { create } from 'zustand';
import type { Node } from '@/types';
import { nodesApi } from '@/api';

interface NodesState {
  nodes: Node[];
  loading: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  add: (data: Parameters<typeof nodesApi.add>[0]) => Promise<Node>;
  remove: (id: string) => Promise<void>;
  updateStatus: (
    updates: Array<{ id: string; status: string; last_seen?: number }>,
  ) => void;
}

export const useNodesStore = create<NodesState>((set, get) => ({
  nodes: [],
  loading: false,
  error: null,

  fetch: async () => {
    set({ loading: true, error: null });
    try {
      const nodes = await nodesApi.list();
      set({ nodes, loading: false });
    } catch (err) {
      set({ error: (err as Error).message, loading: false });
    }
  },

  add: async (data) => {
    const node = await nodesApi.add(data);
    set({ nodes: [...get().nodes, node] });
    return node;
  },

  remove: async (id) => {
    await nodesApi.delete(id);
    set({ nodes: get().nodes.filter((n) => n.id !== id) });
  },

  updateStatus: (updates) => {
    set({
      nodes: get().nodes.map((node) => {
        const update = updates.find((u) => u.id === node.id);
        if (update) {
          return {
            ...node,
            status: update.status as Node['status'],
            last_seen: update.last_seen
              ? new Date(update.last_seen * 1000).toISOString()
              : node.last_seen,
          };
        }
        return node;
      }),
    });
  },
}));
