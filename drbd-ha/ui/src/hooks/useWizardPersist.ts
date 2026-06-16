import { useEffect, useRef } from 'react';
import type { WizardFormInstance } from '@/lib/wizard-form';

interface PersistOptions {
  form: WizardFormInstance;
  storageKey: string;
  enabled?: boolean;
  onSave?: (data: unknown) => void;
  onRestore?: (data: unknown) => void;
}

interface WizardState {
  step: number;
  formData: Record<string, unknown>;
  additionalData?: Record<string, unknown>;
  timestamp: number;
}

/**
 * Hook to persist and restore wizard form state to sessionStorage.
 * Prevents data loss when page is refreshed or navigated away.
 */
export function useWizardPersist({
  form,
  storageKey,
  enabled = true,
  onSave,
  onRestore,
}: PersistOptions) {
  const saveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isRestoringRef = useRef(false);

  // Save state to sessionStorage
  const saveState = (additionalData?: Record<string, unknown>) => {
    if (!enabled) return;

    try {
      const formData = form.getFieldsValue();

      const state: WizardState = {
        step: 0, // Will be updated by caller
        formData,
        additionalData,
        timestamp: Date.now(),
      };

      sessionStorage.setItem(storageKey, JSON.stringify(state));
      onSave?.(state);
    } catch (e) {
      console.warn('Failed to save wizard state:', e);
    }
  };

  // Restore state from sessionStorage
  const restoreState = (): WizardState | null => {
    if (!enabled) return null;

    try {
      const saved = sessionStorage.getItem(storageKey);
      if (!saved) return null;

      const state: WizardState = JSON.parse(saved);

      // Check if state is too old (more than 24 hours)
      const MAX_AGE = 24 * 60 * 60 * 1000;
      if (Date.now() - state.timestamp > MAX_AGE) {
        sessionStorage.removeItem(storageKey);
        return null;
      }

      isRestoringRef.current = true;
      form.setFieldsValue(state.formData);
      onRestore?.(state);

      return state;
    } catch (e) {
      console.warn('Failed to restore wizard state:', e);
      return null;
    } finally {
      // Reset restoring flag after a delay
      setTimeout(() => {
        isRestoringRef.current = false;
      }, 100);
    }
  };

  // Clear saved state
  const clearState = () => {
    sessionStorage.removeItem(storageKey);
  };

  // Debounced save on form values change
  useEffect(() => {
    if (!enabled || isRestoringRef.current) return;

    // Clear previous timeout
    if (saveTimeoutRef.current) {
      clearTimeout(saveTimeoutRef.current);
    }

    // Set new timeout (debounce for 500ms)
    saveTimeoutRef.current = setTimeout(() => {
      saveState();
    }, 500);

    return () => {
      if (saveTimeoutRef.current) {
        clearTimeout(saveTimeoutRef.current);
      }
    };
  }, [enabled, saveState]); // Watch form values

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (saveTimeoutRef.current) {
        clearTimeout(saveTimeoutRef.current);
      }
    };
  }, []);

  return {
    saveState,
    restoreState,
    clearState,
  };
}
