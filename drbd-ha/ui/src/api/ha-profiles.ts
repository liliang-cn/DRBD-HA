import type {
  CreateHaProfileRequest,
  HaProfile,
  HaProfileStatus,
} from '@/types';
import { api } from './client';

export const haProfilesApi = {
  list: () => api.get<{ profiles: HaProfile[] }>('/ha/profiles'),

  get: (id: string) => api.get<HaProfile>(`/ha/profiles/${id}`),

  create: (data: CreateHaProfileRequest) =>
    api.post<{ profile: HaProfile; config_path: string; message: string; promoter_config_content?: string }>(
      '/ha/profiles',
      data,
    ),

  delete: (id: string, deleteResource?: boolean) =>
    api.delete<void>(
      `/ha/profiles/${id}${deleteResource ? '?delete_resource=true' : ''}`,
    ),

  getStatus: (id: string) =>
    api.get<HaProfileStatus>(`/ha/profiles/${id}/status`),

  activate: (id: string) =>
    api.post<HaProfileStatus>(`/ha/profiles/${id}/activate`),

  deactivate: (id: string) =>
    api.post<HaProfileStatus>(`/ha/profiles/${id}/deactivate`),

  evict: (
    id: string,
    options?: {
      node?: string;
      delay?: number;
      keep_masked?: boolean;
      force?: boolean;
    },
  ) =>
    api.post<{
      success: boolean;
      node: string;
      profile: string;
      message: string;
    }>(`/ha/profiles/${id}/evict`, options),

  // drbd-reactor management
  getReactorStatus: () =>
    api.get<{
      service: string;
      active_state: string;
      sub_state: string;
      running: boolean;
    }>('/ha/reactor/status'),

  reloadReactor: () =>
    api.post<{ success: boolean; message: string }>('/ha/reactor/reload'),

  // VIP management
  addVip: (
    id: string,
    vip: { address: string; netmask: number; interface: string },
  ) => api.post<{ message: string }>(`/ha/profiles/${id}/vip`, vip),

  removeVip: (id: string) =>
    api.delete<{ message: string }>(`/ha/profiles/${id}/vip`),

  // Discovery and Import
  getUnmanaged: () => api.get<HaProfile[]>('/ha/unmanaged'),

  importProfiles: (names: string[]) =>
    api.post<{ imported: string[]; failed: string[] }>('/ha/import', { names }),
};
