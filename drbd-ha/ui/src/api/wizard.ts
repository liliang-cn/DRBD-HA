import { api } from './client';

export interface WizardSession {
  id: string;
  mode: 'service' | 'storage';
  current_step: number;
  step_data: Record<string, any>;
  created_at: string;
  updated_at: string;
}

export interface WizardSessionRequest {
  mode: 'service' | 'storage';
  current_step: number;
  step_data: Record<string, any>;
}

export interface WizardStepData {
  step: number;
  data: Record<string, any>;
}

export const wizardApi = {
  // List wizard sessions
  listSessions: async (params?: {
    mode?: 'service' | 'storage';
    limit?: number;
  }): Promise<WizardSession[]> => {
    const queryParams = new URLSearchParams();
    if (params?.mode) queryParams.append('mode', params.mode);
    if (params?.limit) queryParams.append('limit', params.limit.toString());

    const response = await api.get<WizardSession[]>(
      `/wizard/sessions?${queryParams.toString()}`,
    );
    return response.data;
  },

  // Create a new wizard session
  createSession: async (
    request: WizardSessionRequest,
  ): Promise<WizardSession> => {
    const response = await api.post<WizardSession>('/wizard/sessions', request);
    return response.data;
  },

  // Get a wizard session by ID
  getSession: async (id: string): Promise<WizardSession> => {
    const response = await api.get<WizardSession>(`/wizard/sessions/${id}`);
    return response.data;
  },

  // Update a wizard session
  updateSession: async (
    id: string,
    request: WizardSessionRequest,
  ): Promise<WizardSession> => {
    const response = await api.put<WizardSession>(
      `/wizard/sessions/${id}`,
      request,
    );
    return response.data;
  },

  // Delete a wizard session
  deleteSession: async (id: string): Promise<void> => {
    await api.delete(`/wizard/sessions/${id}`);
  },

  // Save progress for a specific step
  saveStep: async (
    id: string,
    step: number,
    data: Record<string, any>,
  ): Promise<WizardSession> => {
    const response = await api.post<WizardSession>(
      `/wizard/sessions/${id}/step/${step}`,
      data,
    );
    return response.data;
  },
};
