import { create } from 'zustand';
import { haProfilesApi } from '@/api';
import type { HaProfile } from '@/types';

interface HaProfilesState {
  profiles: HaProfile[];
  loading: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  updateProfileStatus: (id: string, status: string) => void;
}

export const useHaProfilesStore = create<HaProfilesState>((set, get) => ({
  profiles: [],
  loading: false,
  error: null,

  fetch: async () => {
    set({ loading: true, error: null });
    try {
      const { profiles } = await haProfilesApi.list();
      set({ profiles, loading: false });
    } catch (err) {
      set({ error: (err as Error).message, loading: false });
    }
  },

  updateProfileStatus: (id: string, status: string) => {
    set((state) => ({
      profiles: state.profiles.map((p) =>
        p.id === id ? { ...p, status: status as HaProfile['status'] } : p,
      ),
    }));
  },
}));
