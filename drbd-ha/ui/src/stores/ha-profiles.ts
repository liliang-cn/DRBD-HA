import { create } from 'zustand';
import type { HaProfile } from '@/types';
import { haProfilesApi } from '@/api';

interface HaProfilesState {
  profiles: HaProfile[];
  loading: boolean;
  statusLoading: Set<string>; // Profile IDs currently loading status
  error: string | null;
  fetch: () => Promise<void>;
  updateProfileStatus: (id: string, status: string) => void;
  setStatusLoading: (id: string, loading: boolean) => void;
}

export const useHaProfilesStore = create<HaProfilesState>((set, get) => ({
  profiles: [],
  loading: false,
  statusLoading: new Set(),
  error: null,

  fetch: async () => {
    set({ loading: true, error: null });
    try {
      const { profiles } = await haProfilesApi.list();
      // Mark all profiles as loading status
      const loadingIds = new Set(profiles.map((p) => p.id));
      set({ profiles, loading: false, statusLoading: loadingIds });

      // Fetch real-time status for each profile
      profiles.forEach(async (profile) => {
        try {
          const status = await haProfilesApi.getStatus(profile.id);
          get().updateProfileStatus(profile.id, status.status);
        } catch {
          // Ignore status fetch errors
        } finally {
          get().setStatusLoading(profile.id, false);
        }
      });
    } catch (err) {
      set({ error: (err as Error).message, loading: false });
    }
  },

  updateProfileStatus: (id: string, status: string) => {
    set((state) => ({
      profiles: state.profiles.map((p) =>
        p.id === id ? { ...p, status: status as HaProfile['status'] } : p
      ),
    }));
  },

  setStatusLoading: (id: string, loading: boolean) => {
    set((state) => {
      const newSet = new Set(state.statusLoading);
      if (loading) {
        newSet.add(id);
      } else {
        newSet.delete(id);
      }
      return { statusLoading: newSet };
    });
  },
}));
