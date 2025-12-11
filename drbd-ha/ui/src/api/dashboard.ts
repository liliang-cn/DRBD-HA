import type { DashboardSummary } from '@/types';
import { api } from './client';

export const dashboardApi = {
  getSummary: () => api.get<DashboardSummary>('/dashboard/summary'),
};
