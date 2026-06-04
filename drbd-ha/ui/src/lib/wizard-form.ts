// Standalone, dependency-free form-instance store for the wizard steps.
//
// Background: antd has been removed from the project. The previous wizard form
// relied on antd's `Form.useForm()` / `FormInstance`. This module provides a
// minimal, API-compatible replacement covering exactly the surface the wizard
// steps (and `OcfAgentEditor` via its `externalForm` prop) use:
//   getFieldValue / setFieldValue (string or nested path array)
//   getFieldsValue / setFieldsValue / resetFields / validateFields
//   getFieldsError (no-op success)
// plus a `useWizardWatch(name, form)` hook for reactive reads.
import { useEffect, useRef, useState } from 'react';

type Values = Record<string, any>;
type NamePath = string | (string | number)[];

export interface WizardFormInstance {
  getFieldValue: (name: NamePath) => any;
  getFieldsValue: (...args: any[]) => any;
  setFieldValue: (name: NamePath, value: any) => void;
  setFieldsValue: (values: Values) => void;
  resetFields: (fields?: NamePath[]) => void;
  validateFields: (...args: any[]) => Promise<Values>;
  getFieldsError: (...args: any[]) => any[];
  // Internal subscription primitives used by useWizardWatch.
  __subscribe: (cb: () => void) => () => void;
}

function toPath(name: NamePath): (string | number)[] {
  return Array.isArray(name) ? name : [name];
}

function getIn(obj: Values, path: (string | number)[]): any {
  let cur: any = obj;
  for (const key of path) {
    if (cur == null) return undefined;
    cur = cur[key];
  }
  return cur;
}

function setIn(obj: Values, path: (string | number)[], value: any): void {
  let cur: any = obj;
  for (let i = 0; i < path.length - 1; i++) {
    const key = path[i];
    if (cur[key] == null || typeof cur[key] !== 'object') {
      cur[key] = typeof path[i + 1] === 'number' ? [] : {};
    }
    cur = cur[key];
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
    getFieldValue: (name) => getIn(store, toPath(name)),
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
 * Drop-in replacement for antd's `Form.useForm()`. Returns a tuple so existing
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
 * Drop-in replacement for antd's `Form.useWatch(name, form)`. Re-renders the
 * caller whenever the watched field (or any field) changes.
 */
export function useWizardWatch(name: NamePath, form?: WizardFormInstance): any {
  const [, forceRender] = useState({});
  useEffect(() => {
    if (!form) return;
    return form.__subscribe(() => forceRender({}));
  }, [form]);
  if (!form) return undefined;
  return form.getFieldValue(name);
}
