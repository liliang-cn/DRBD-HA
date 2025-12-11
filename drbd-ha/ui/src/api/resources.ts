import type {
  CreateResourceRequest,
  DrbdResource,
  ResourceAction,
} from '@/types';
import { api } from './client';

export const resourcesApi = {
  list: () => api.get<{ resources: DrbdResource[] }>('/resources'),

  get: (name: string) => api.get<DrbdResource>(`/resources/${name}`),

  create: (data: CreateResourceRequest) =>
    api.post<{ name: string; message: string; config_path: string }>(
      '/resources',
      data,
    ),

  delete: (name: string) => api.delete<void>(`/resources/${name}`),

  action: (name: string, action: ResourceAction) =>
    api.post<{
      resource: string;
      action: string;
      success: boolean;
      message: string | null;
    }>(`/resources/${name}/action`, action),

  init: (name: string) =>
    api.post<{
      resource: string;
      action: string;
      success: boolean;
      message: string;
    }>(`/resources/${name}/init`),

  mkfs: (name: string, fstype: string, force = false) =>
    api.post<{
      resource: string;
      action: string;
      success: boolean;
      message: string;
    }>(`/resources/${name}/mkfs`, { fstype, force }),

  mount: (name: string, mount_point: string, options?: string) =>
    api.post<{
      resource: string;
      action: string;
      success: boolean;
      message: string;
    }>(`/resources/${name}/mount`, { mount_point, options }),

  umount: (name: string, mount_point: string) =>
    api.post<{
      resource: string;
      action: string;
      success: boolean;
      message: string;
    }>(`/resources/${name}/umount`, { mount_point }),
};
