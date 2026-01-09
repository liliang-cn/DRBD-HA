import type { AddNodeRequest, BlockDevice, Node } from '@/types';
import { api } from './client';

export const nodesApi = {
  list: () => api.get<Node[]>('/nodes'),

  get: (id: string) => api.get<Node>(`/nodes/${id}`),

  add: (data: AddNodeRequest) => api.post<Node>('/nodes', data),

  update: (id: string, data: AddNodeRequest) => api.put<Node>(`/nodes/${id}`, data),

  delete: (id: string) => api.delete<void>(`/nodes/${id}`),

  getDisks: (id: string) => api.get<BlockDevice[]>(`/nodes/${id}/disks`),

  getAvailableDisks: (id: string) =>
    api.get<BlockDevice[]>(`/nodes/${id}/disks/available`),

  check: (id: string) =>
    api.post<{
      id: string;
      hostname: string;
      status: string;
      message: string | null;
    }>(`/nodes/${id}/check`),
};
