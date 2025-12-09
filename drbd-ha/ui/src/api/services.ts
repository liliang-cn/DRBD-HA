import { api } from './client';
import type { ServiceInfo, ServiceFileInfo } from '@/types';

export const servicesApi = {
  list: (includeSystem = false) =>
    api.get<{ services: ServiceInfo[] }>(`/services${includeSystem ? '?include_system=true' : ''}`),

  listAvailable: (includeSystem = false) =>
    api.get<{ services: ServiceFileInfo[] }>(`/services/available${includeSystem ? '?include_system=true' : ''}`),
};
