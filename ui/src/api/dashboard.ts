import { api } from './client';
import type { DashboardSummary } from '@/types';

export const dashboardApi = {
  getSummary: () => api.get<DashboardSummary>('/dashboard/summary'),
};
