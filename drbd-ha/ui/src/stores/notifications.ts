import { create } from 'zustand';
import type { NotificationEvent, ProgressEvent } from '@/types';

interface NotificationsState {
  notifications: NotificationEvent[];
  progress: ProgressEvent[];
  addNotification: (notification: NotificationEvent) => void;
  updateProgress: (progress: ProgressEvent) => void;
  clearNotifications: () => void;
}

export const useNotificationsStore = create<NotificationsState>((set, get) => ({
  notifications: [],
  progress: [],

  addNotification: (notification) => {
    set({ notifications: [...get().notifications.slice(-99), notification] });
  },

  updateProgress: (progress) => {
    const existing = get().progress;
    const index = existing.findIndex(
      (p) => p.operation_id === progress.operation_id,
    );
    if (index >= 0) {
      const updated = [...existing];
      updated[index] = progress;
      set({ progress: updated });
    } else {
      set({ progress: [...existing, progress] });
    }
    // Remove completed progress after 5 seconds
    if (progress.completed) {
      setTimeout(() => {
        set({
          progress: get().progress.filter(
            (p) => p.operation_id !== progress.operation_id,
          ),
        });
      }, 5000);
    }
  },

  clearNotifications: () => set({ notifications: [] }),
}));
