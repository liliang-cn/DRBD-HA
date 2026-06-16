// Standalone, dependency-free form-instance store for the wizard steps.
//
// A shared form instance passed across the wizard steps (and into
// `OcfAgentEditor` via its `externalForm` prop). Surface area:
//   getFieldValue / setFieldValue (string or nested path array)
//   getFieldsValue / setFieldsValue / resetFields / validateFields
//   getFieldsError (no-op success)
// plus a `useWizardWatch(name, form)` hook for reactive reads.
import { useEffect, useRef, useState } from 'react';

type Values = Record<string, unknown>;
type NamePath = string | (string | number)[];

export interface WizardFormInstance {
  /** Read a field. Defaults to `unknown`; pass `T` to assert the shape. */
  getFieldValue: <T = unknown>(name: NamePath) => T;
  getFieldsValue: () => Values;
  setFieldValue: (name: NamePath, value: unknown) => void;
  setFieldsValue: (values: Values) => void;
  resetFields: (fields?: NamePath[]) => void;
  validateFields: () => Promise<Values>;
  getFieldsError: () => unknown[];
  // Internal subscription primitives used by useWizardWatch.
  __subscribe: (cb: () => void) => () => void;
}

function toPath(name: NamePath): (string | number)[] {
  return Array.isArray(name) ? name : [name];
}

function getIn(obj: Values, path: (string | number)[]): unknown {
  let cur: unknown = obj;
  for (const key of path) {
    if (cur == null || typeof cur !== 'object') return undefined;
    cur = (cur as Record<string | number, unknown>)[key];
  }
  return cur;
}

function setIn(obj: Values, path: (string | number)[], value: unknown): void {
  let cur: Record<string | number, unknown> = obj;
  for (let i = 0; i < path.length - 1; i++) {
    const key = path[i];
    if (cur[key] == null || typeof cur[key] !== 'object') {
      cur[key] = typeof path[i + 1] === 'number' ? [] : {};
    }
    cur = cur[key] as Record<string | number, unknown>;
  }
  cur[path[path.length - 1]] = value;
}

function createForm(): WizardFormInstance {
  const store: Values = {};
  const listeners = new Set<() => void>();

  const notify = () => {
    listeners.forEach((cb) => cb());
  };

  const form: WizardFormInstance = {
    getFieldValue: <T = unknown>(name: NamePath) =>
      getIn(store, toPath(name)) as T,
    getFieldsValue: () => store,
    setFieldValue: (name, value) => {
      setIn(store, toPath(name), value);
      notify();
    },
    setFieldsValue: (values) => {
      Object.entries(values || {}).forEach(([k, v]) => {
        store[k] = v;
      });
      notify();
    },
    resetFields: (fields) => {
      if (fields && fields.length > 0) {
        fields.forEach((f) => {
          const path = toPath(f);
          if (path.length === 1) delete store[path[0]];
          else setIn(store, path, undefined);
        });
      } else {
        Object.keys(store).forEach((k) => delete store[k]);
      }
      notify();
    },
    // No client-side validation rules are tracked; resolve with current values.
    validateFields: () => Promise.resolve({ ...store }),
    getFieldsError: () => [],
    __subscribe: (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };

  return form;
}

/**
 * Returns a tuple so existing
 * `const [form] = useWizardForm()` call sites keep working.
 */
export function useWizardForm(): [WizardFormInstance] {
  const ref = useRef<WizardFormInstance | null>(null);
  if (!ref.current) {
    ref.current = createForm();
  }
  return [ref.current];
}

/**
 * Reactive field reader. Re-renders the caller whenever the watched field
 * (or any field) changes.
 */
export function useWizardWatch(
  name: NamePath,
  form?: WizardFormInstance,
): unknown {
  const [, forceRender] = useState({});
  useEffect(() => {
    if (!form) return;
    return form.__subscribe(() => forceRender({}));
  }, [form]);
  if (!form) return undefined;
  return form.getFieldValue(name);
}
