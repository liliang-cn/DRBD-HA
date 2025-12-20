import { useCallback, useEffect, useState } from 'react';
import {
  type WizardSession,
  type WizardSessionRequest,
  wizardApi,
} from '@/api';

export interface UseWizardSessionOptions {
  mode: 'service' | 'storage';
  sessionId?: string;
}

export interface UseWizardSessionReturn {
  session: WizardSession | null;
  loading: boolean;
  error: string | null;
  createSession: () => Promise<WizardSession>;
  updateSession: (
    step: number,
    data: Record<string, any>,
  ) => Promise<WizardSession>;
  saveStep: (step: number, data: Record<string, any>) => Promise<WizardSession>;
  loadSession: (id: string) => Promise<WizardSession | null>;
  getStepData: (step: number) => Record<string, any>;
  clearSession: () => void;
  getRecentSessions: () => Promise<WizardSession[]>;
}

export const useWizardSession = ({
  mode,
  sessionId,
}: UseWizardSessionOptions): UseWizardSessionReturn => {
  const [session, setSession] = useState<WizardSession | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const createSession = useCallback(async (): Promise<WizardSession> => {
    setLoading(true);
    setError(null);

    try {
      const request: WizardSessionRequest = {
        mode,
        current_step: 0,
        step_data: {},
      };

      const newSession = await wizardApi.createSession(request);
      setSession(newSession);
      return newSession;
    } catch (err) {
      const errorMessage =
        err instanceof Error ? err.message : 'Failed to create wizard session';
      setError(errorMessage);
      throw new Error(errorMessage);
    } finally {
      setLoading(false);
    }
  }, [mode]);

  const updateSession = useCallback(
    async (step: number, data: Record<string, any>): Promise<WizardSession> => {
      if (!session) {
        throw new Error('No active session');
      }

      setLoading(true);
      setError(null);

      try {
        // Merge step data with existing session data
        const updatedStepData = {
          ...session.step_data,
          [`step_${step}`]: data,
        };

        const request: WizardSessionRequest = {
          mode: session.mode,
          current_step: step,
          step_data: updatedStepData,
        };

        const updatedSession = await wizardApi.updateSession(
          session.id,
          request,
        );
        setSession(updatedSession);
        return updatedSession;
      } catch (err) {
        const errorMessage =
          err instanceof Error
            ? err.message
            : 'Failed to update wizard session';
        setError(errorMessage);
        throw new Error(errorMessage);
      } finally {
        setLoading(false);
      }
    },
    [session],
  );

  const saveStep = useCallback(
    async (step: number, data: Record<string, any>): Promise<WizardSession> => {
      if (!session) {
        throw new Error('No active session');
      }

      setLoading(true);
      setError(null);

      try {
        const updatedSession = await wizardApi.saveStep(session.id, step, data);
        setSession(updatedSession);
        return updatedSession;
      } catch (err) {
        const errorMessage =
          err instanceof Error ? err.message : 'Failed to save wizard step';
        setError(errorMessage);
        throw new Error(errorMessage);
      } finally {
        setLoading(false);
      }
    },
    [session],
  );

  const loadSession = useCallback(
    async (id: string): Promise<WizardSession | null> => {
      setLoading(true);
      setError(null);

      try {
        const loadedSession = await wizardApi.getSession(id);
        setSession(loadedSession);
        return loadedSession;
      } catch (err) {
        const errorMessage =
          err instanceof Error ? err.message : 'Failed to load wizard session';
        setError(errorMessage);
        return null;
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  const getStepData = useCallback(
    (step: number): Record<string, any> => {
      if (!session?.step_data) return {};
      return session.step_data[`step_${step}`] || {};
    },
    [session?.step_data],
  );

  const clearSession = useCallback(() => {
    setSession(null);
    setError(null);
  }, []);

  const getRecentSessions = useCallback(async (): Promise<WizardSession[]> => {
    try {
      const sessions = await wizardApi.listSessions({ mode, limit: 10 });
      return sessions || [];
    } catch (err) {
      console.error('Failed to fetch recent sessions:', err);
      return [];
    }
  }, [mode]);

  // Load session by ID if provided
  useEffect(() => {
    if (sessionId) {
      loadSession(sessionId);
    }
  }, [sessionId, loadSession]);

  return {
    session,
    loading,
    error,
    createSession,
    updateSession,
    saveStep,
    loadSession,
    getStepData,
    clearSession,
    getRecentSessions,
  };
};
