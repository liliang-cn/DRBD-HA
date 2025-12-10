import { api } from './client';
import type { Node, AddNodeRequest, BlockDevice } from '@/types';

export const nodesApi = {
  list: () => api.get<Node[]>('/nodes'),

  get: (id: string) => api.get<Node>(`/nodes/${id}`),

  add: (data: AddNodeRequest) => api.post<Node>('/nodes', data),

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
