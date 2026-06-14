import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
} from '@dnd-kit/core';
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { ClipboardPaste, Eye, EyeOff, Plus, RotateCw, Save } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
import { haProfilesApi } from '@/api';
import type {
  OcfAgentWithMetadata,
  ParamEntry,
  ParsedOcfAgent,
  ResourceAgentsByProvider,
} from '@/api/ha-profiles';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Empty } from '@/components/ui/empty';
import { Spinner } from '@/components/ui/spinner';
import {
  useWizardForm,
  type WizardFormInstance,
} from '@/lib/wizard-form';
import { useThemeStore } from '@/stores/theme';
import { extractStartArrayItems, parseOcfLine } from '@/utils/toml';
import { AddAgentModal } from './ocf-agent-editor/AddAgentModal';
import { AddParameterModal } from './ocf-agent-editor/AddParameterModal';
import { AgentPreview } from './ocf-agent-editor/AgentPreview';
import { PasteTomlModal } from './ocf-agent-editor/PasteTomlModal';
import { SortableAgentItem } from './ocf-agent-editor/SortableAgentItem';

// Helper functions to convert between ParamEntry[] and Record<string, string>
function paramsToRecord(params: ParamEntry[]): Record<string, string> {
  const record: Record<string, string> = {};
  params.forEach(({ key, value }) => {
    record[key] = value;
  });
  return record;
}

interface OcfAgentEditorProps {
  // For edit mode
  profile?: { name: string; id: string } | null;
  onSave?: () => void;
  onCancel?: () => void;

  // For create/wizard mode
  mode?: 'edit' | 'create';
  externalForm?: WizardFormInstance; // External form for create mode
  resources?: { name: string }[]; // Available resources for create mode
  services?: string[]; // Available services for create mode
  onAgentsChange?: (agents: any[]) => void; // Callback when agents change

  // Preview control
  showPreview?: boolean; // Controlled from outside
  onPreviewChange?: (show: boolean) => void; // Callback when preview toggle changes

  // Dirty tracking (unsaved changes)
  onDirtyChange?: (dirty: boolean) => void;
}

export function OcfAgentEditor({
  profile,
  onSave,
  onCancel,
  mode = 'edit',
  externalForm,
  onAgentsChange,
  showPreview: externalShowPreview,
  onPreviewChange,
  onDirtyChange,
}: OcfAgentEditorProps) {
  const { theme: currentTheme } = useThemeStore();
  const [internalForm] = useWizardForm();

  // Use external form in create mode, internal form in edit mode
  const form = externalForm || internalForm;

  // Preview visibility state (default hidden in create mode, shown in edit mode)
  const [previewVisible, setPreviewVisible] = useState(mode === 'edit');

  // Toggle preview visibility
  const togglePreview = useCallback(() => {
    setPreviewVisible((prev) => {
      const newValue = !prev;
      onPreviewChange?.(newValue);
      return newValue;
    });
  }, [onPreviewChange]);

  // Sync external showPreview prop
  useEffect(() => {
    if (externalShowPreview !== undefined) {
      setPreviewVisible(externalShowPreview);
    }
  }, [externalShowPreview]);

  const loadingRef = useRef(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  // 解析的 OCF agents（来自 start 数组）
  const [parsedAgents, setParsedAgents] = useState<OcfAgentWithMetadata[]>([]);

  // Original TOML content and resource name
  const [_originalToml, setOriginalToml] = useState<string>('');
  const [_resourceName, setResourceName] = useState<string>('');

  // Form values for live preview
  const [, forceUpdate] = useState({});

  // 所有可用的 resource agents（按 provider 分组）
  const [allAgents, setAllAgents] = useState<ResourceAgentsByProvider | null>(
    null,
  );

  // 展开的 agent keys
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());

  // Dirty (unsaved changes) tracking — marked on any mutation, cleared on
  // load/save. Exposed to the parent via onDirtyChange.
  const dirtyRef = useRef(false);
  const markDirty = useCallback(() => {
    if (!dirtyRef.current) {
      dirtyRef.current = true;
      onDirtyChange?.(true);
    }
  }, [onDirtyChange]);
  const clearDirty = useCallback(() => {
    if (dirtyRef.current) {
      dirtyRef.current = false;
      onDirtyChange?.(false);
    }
  }, [onDirtyChange]);

  // Paste TOML modal state
  const [pasteTomlVisible, setPasteTomlVisible] = useState(false);

  // Add agent modal state
  const [addModalVisible, setAddModalVisible] = useState(false);
  const [addAgentType, setAddAgentType] = useState<'ocf' | 'systemd'>('ocf');
  const [selectedProvider, setSelectedProvider] = useState<string>('');
  const [selectedAgent, setSelectedAgent] = useState<string>('');
  const [systemdUnit, setSystemdUnit] = useState<string>('');

  // Split pane state for resizable panels
  const [leftPanelWidth, setLeftPanelWidth] = useState(50); // percentage
  const [_isDragging, setIsDragging] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const startXRef = useRef(0);
  const startWidthRef = useRef(50);

  // Handle drag start
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
    startXRef.current = e.clientX;
    startWidthRef.current = leftPanelWidth;

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  };

  // Handle drag move
  const handleMouseMove = (e: MouseEvent) => {
    if (!containerRef.current) return;

    const containerRect = containerRef.current.getBoundingClientRect();
    const containerWidth = containerRect.width;
    const deltaX = e.clientX - startXRef.current;
    const deltaPercent = (deltaX / containerWidth) * 100;

    let newWidth = startWidthRef.current + deltaPercent;
    newWidth = Math.max(20, Math.min(80, newWidth));

    setLeftPanelWidth(newWidth);
  };

  // Handle drag end
  const handleMouseUp = () => {
    setIsDragging(false);
    document.removeEventListener('mousemove', handleMouseMove);
    document.removeEventListener('mouseup', handleMouseUp);
  };

  // Track manually added parameters for each agent
  // Key is a unique instance ID that never changes
  const [addedParams, setAddedParams] = useState<Map<number, Set<string>>>(
    new Map(),
  );

  // Counter for generating unique instance IDs for agents
  const [nextInstanceId, setNextInstanceId] = useState(0);

  // Add parameter modal state
  const [addParamModalVisible, setAddParamModalVisible] = useState(false);
  const [currentAgentIndex, setCurrentAgentIndex] = useState<number | null>(
    null,
  );
  const [selectedParam, setSelectedParam] = useState<string>('');

  // DnD sensors
  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const loadParsedAgents = useCallback(async () => {
    // In create mode, load from form
    if (mode === 'create') {
      const ocfAgents = form.getFieldValue('ocf_agents') || [];
      if (ocfAgents.length > 0) {
        // Convert form data to parsed agents format
        let instanceIdCounter = 0;
        const agentsWithIds = ocfAgents.map((agentData: any) => {
          // Check if it's OCF agent or plain systemd unit
          if (agentData.type === 'ocf') {
            // OCF agent
            const { provider, agent_type, instance_name, params } = agentData;
            const original = `ocf:${provider}:${agent_type} ${instance_name}`;

            // Build param string
            const paramStr = Object.entries(params || {})
              .filter(([_, value]) => value !== undefined && value !== '')
              .map(([key, value]) => {
                if (
                  String(value).includes(' ') ||
                  String(value).includes(',') ||
                  String(value) === ''
                ) {
                  return `${key}='${value}'`;
                }
                return `${key}=${value}`;
              })
              .join(' ');

            const fullOriginal = paramStr
              ? `${original} ${paramStr}`
              : original;

            return {
              position: {
                section: 'resources',
                array_index: null,
                key: 'start',
                index: instanceIdCounter,
              },
              item: {
                original: fullOriginal,
                is_ocf: true,
                ocf_agent: {
                  original: fullOriginal,
                  provider,
                  agent_type,
                  instance_name,
                  params: params || {},
                },
              },
              metadata: null, // Will load later
              instanceId: instanceIdCounter++,
            };
          } else if (agentData.type === 'mount') {
            // Mount unit
            return {
              position: {
                section: 'resources',
                array_index: null,
                key: 'start',
                index: instanceIdCounter,
              },
              item: {
                original: agentData.value || '',
                is_ocf: false,
                ocf_agent: null,
              },
              metadata: null,
              instanceId: instanceIdCounter++,
            };
          } else {
            // Service
            return {
              position: {
                section: 'resources',
                array_index: null,
                key: 'start',
                index: instanceIdCounter,
              },
              item: {
                original: agentData.value || '',
                is_ocf: false,
                ocf_agent: null,
              },
              metadata: null,
              instanceId: instanceIdCounter++,
            };
          }
        });

        setParsedAgents(agentsWithIds);
        setNextInstanceId(instanceIdCounter);

        // Initialize form with agent data
        const initialValues = {
          agents: agentsWithIds.map((agentWithMeta: OcfAgentWithMetadata) => {
            if (agentWithMeta.item.is_ocf && agentWithMeta.item.ocf_agent) {
              return {
                params: paramsToRecord(
                  agentWithMeta.item.ocf_agent.params || [],
                ),
                original: agentWithMeta.item.original,
              };
            } else {
              return {
                original: agentWithMeta.item.original,
              };
            }
          }),
        };

        setTimeout(() => {
          form.setFieldsValue(initialValues);
        }, 50);
      }
      return;
    }

    // Edit mode - load from API
    if (!profile) return;

    // 防止重复调用（React.StrictMode会导致effect执行两次）
    if (loadingRef.current) return;
    loadingRef.current = true;

    setLoading(true);

    try {
      // Load both parsed agents and original TOML
      const [parseResult, tomlResult] = await Promise.all([
        haProfilesApi.parseToml(profile.name),
        haProfilesApi.getToml(profile.name),
      ]);

      // Save original TOML content
      setOriginalToml(tomlResult.content);

      // Extract resource name from parsed agents (first agent's section)
      if (parseResult.content.ocf_agents.length > 0) {
        const firstAgent = parseResult.content.ocf_agents[0];
        // Extract resource name from position like "linstor_db"
        setResourceName(firstAgent.position.section);
      }

      // 只取 start 数组中的 agents
      const startAgents = parseResult.content.ocf_agents.filter(
        (agent) => agent.position.key === 'start',
      );

      // Assign unique instance IDs to all agents (stored as a hidden property)
      let instanceIdCounter = 0;
      const agentsWithIds = startAgents.map((agent) => ({
        ...agent,
        instanceId: instanceIdCounter++, // Add instance ID property
      }));

      setParsedAgents(agentsWithIds);
      setNextInstanceId(instanceIdCounter); // Next new agent will use this

      // Initialize form with agent data (use agentsWithIds, not startAgents)
      const initialValues = {
        agents: agentsWithIds.map((agentWithMeta) => {
          if (agentWithMeta.item.is_ocf && agentWithMeta.item.ocf_agent) {
            return {
              params: paramsToRecord(agentWithMeta.item.ocf_agent.params || []),
              original: agentWithMeta.item.original,
            };
          } else {
            return {
              original: agentWithMeta.item.original,
            };
          }
        }),
      };

      // Wait for React to render the fields before setting values
      setTimeout(() => {
        form.setFieldsValue(initialValues);
      }, 50);
      clearDirty();
    } catch (err) {
      toast.error((err as { message: string }).message);
    } finally {
      loadingRef.current = false;
      setLoading(false);
    }
  }, [mode, form, profile, clearDirty]);

  const loadAllResourceAgents = useCallback(async () => {
    try {
      // Try to load from localStorage cache first
      const cacheKey = 'ha-profiles:resource-agents';
      const cachedData = localStorage.getItem(cacheKey);
      const cacheTimestamp = localStorage.getItem(`${cacheKey}:timestamp`);

      // Cache is valid for 1 hour
      const CACHE_DURATION = 60 * 60 * 1000;
      const now = Date.now();

      if (cachedData && cacheTimestamp) {
        const timestamp = parseInt(cacheTimestamp, 10);
        if (now - timestamp < CACHE_DURATION) {
          // Use cached data
          try {
            const parsed = JSON.parse(cachedData);
            setAllAgents(parsed);
            return;
          } catch (e) {
            console.warn(
              'Failed to parse cached data, will fetch from backend',
              e,
            );
            // Clear invalid cache
            localStorage.removeItem(cacheKey);
            localStorage.removeItem(`${cacheKey}:timestamp`);
          }
        }
      }

      // No valid cache, fetch from backend
      const result = await haProfilesApi.getAllResourceAgents();
      setAllAgents(result);

      // Save to localStorage cache
      localStorage.setItem(cacheKey, JSON.stringify(result));
      localStorage.setItem(`${cacheKey}:timestamp`, now.toString());
    } catch (err) {
      console.error('Failed to load all resource agents:', err);
      toast.warning('Failed to load agent metadata');
      // Set to empty object to prevent infinite loading state
      setAllAgents({ providers: {} } as any);
    }
  }, []);

  // Sync parsedAgents back to parent form (for create mode)
  const syncToParentForm = useCallback(() => {
    if (mode === 'create' && externalForm) {
      // Convert parsedAgents back to ocf_agents format (backend expects)
      const ocfAgents = parsedAgents
        .filter(
          (agentWithMeta: any) =>
            agentWithMeta.item.is_ocf && agentWithMeta.item.ocf_agent,
        )
        .map((agentWithMeta: any) => {
          const ocfAgent = agentWithMeta.item.ocf_agent;
          return {
            name: `ocf:${ocfAgent.provider}:${ocfAgent.agent_type}`,
            instance_name: ocfAgent.instance_name,
            params: ocfAgent.params || {},
          };
        });

      externalForm.setFieldValue('ocf_agents', ocfAgents);
      // Also call the callback to notify parent component
      onAgentsChange?.(ocfAgents);
    }
  }, [mode, externalForm, onAgentsChange, parsedAgents]);

  // 加载 TOML 并解析 OCF agents / 加载 form 数据
  useEffect(() => {
    if (mode === 'create') {
      loadParsedAgents();
    } else if (profile) {
      loadParsedAgents();
    }
  }, [profile, mode, loadParsedAgents]);

  // Sync changes to parent form in create mode
  useEffect(() => {
    if (mode === 'create') {
      syncToParentForm();
    }
  }, [syncToParentForm, mode]);

  // 加载所有可用的 resource agents（异步，只加载一次）
  useEffect(() => {
    // Load in edit mode when we have agents, or in create mode immediately
    if (!allAgents) {
      if (mode === 'create' || parsedAgents.length > 0) {
        loadAllResourceAgents();
      }
    }
  }, [parsedAgents.length, allAgents, mode, loadAllResourceAgents]);

  const handleSave = async () => {
    // In create mode, just sync and call callback
    if (mode === 'create') {
      syncToParentForm();
      onSave?.();
      return;
    }

    // Edit mode - call API
    if (!profile) return;

    setSaving(true);
    try {
      // Get current form values (with fallback)
      let values = form.getFieldsValue();

      // Try to validate, but use current values if validation fails
      try {
        values = await form.validateFields();
      } catch (validationError) {
        console.warn(
          'Form validation failed, using current values:',
          validationError,
        );
      }

      // Check if agents array exists (optional now, since we use parsedAgents as source of truth)
      const formAgents = values.agents || [];

      // Generate the updated start array strings using parsedAgents as source of truth
      // This ensures all agents are included even if form doesn't have complete data
      const startArrayItems = parsedAgents.map(
        (agentWithMeta: any, index: number) => {
          const item = agentWithMeta.item;
          // Get form data for this agent if available
          const agentData = formAgents[index] || {};

          if (item.is_ocf && item.ocf_agent) {
            // OCF agent - generate string from parsedAgent (source of truth)
            // Form values are used to update params if user changed them
            const formParams = agentData.params || {};
            const originalParams = paramsToRecord(item.ocf_agent.params || []);
            // Merge: form params override original params
            const mergedParams = { ...originalParams, ...formParams };
            const agent = item.ocf_agent;

            // Generate parameter string (empty if no params)
            const paramStr = Object.entries(mergedParams)
              .filter(([_, value]) => value !== undefined && value !== '')
              .map(([key, value]) => {
                if (
                  String(value).includes(' ') ||
                  String(value).includes(',') ||
                  String(value) === ''
                ) {
                  return `${key}='${value}'`;
                }
                return `${key}=${value}`;
              })
              .join(' ');

            return `ocf:${agent.provider}:${agent.agent_type} ${agent.instance_name}${paramStr ? ` ${paramStr}` : ''}`;
          } else {
            // Non-OCF item - use original from parsedAgent
            return item.original;
          }
        },
      );

      // Step 1: Update start array using the new API (automatically syncs and reloads drbd-reactor)
      toast.loading('Saving and syncing configuration...');
      const result = await haProfilesApi.updateStartArray(
        profile.name,
        startArrayItems,
      );

      if (result.success) {
        toast.success(
          result.message ||
            `Configuration saved and synced to ${result.synced_nodes.length} node(s)`,
        );
      } else {
        toast.warning(
          result.message || 'Configuration saved but sync had issues',
        );
      }

      // Reload the parsed agents to reflect changes
      await loadParsedAgents();
      clearDirty();

      onSave?.();
    } catch (err) {
      console.error('Save failed:', err);
      toast.error(`Failed to save: ${(err as { message: string }).message}`);
    } finally {
      setSaving(false);
    }
  };

  // 根据解析的 agent 查找匹配的元数据
  const findAgentMetadata = (agent: ParsedOcfAgent) => {
    if (!allAgents) return null;

    const providerAgents = allAgents.providers[agent.provider];
    if (!providerAgents) return null;

    return providerAgents.find((a) => a.name === agent.agent_type) || null;
  };

  // 拖拽结束处理
  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;

    if (over && active.id !== over.id) {
      // Find indices based on instanceId in the ID string
      const oldIndex = parsedAgents.findIndex(
        (a) => `agent-${a.instanceId}` === active.id,
      );
      const newIndex = parsedAgents.findIndex(
        (a) => `agent-${a.instanceId}` === over.id,
      );

      if (oldIndex >= 0 && newIndex >= 0) {
        // Move parsedAgents array - this has the latest data from handleFormValuesChange
        const newAgents = arrayMove(parsedAgents, oldIndex, newIndex);
        setParsedAgents(newAgents);

        // Rebuild form data from parsedAgents to ensure consistency
        // Don't rely on form.getFieldsValue() as it may be incomplete when panels are collapsed
        const newFormAgents = newAgents.map((agentWithMeta) => {
          if (agentWithMeta.item.is_ocf && agentWithMeta.item.ocf_agent) {
            return {
              params: paramsToRecord(agentWithMeta.item.ocf_agent.params || []),
              original: agentWithMeta.item.original,
            };
          } else {
            return {
              original: agentWithMeta.item.original,
            };
          }
        });

        form.setFieldValue('agents', newFormAgents);
        markDirty();
      }
    }
  };

  // Apply pasted TOML: replace the agent list with entries parsed from the
  // pasted content's start array. Returns an error string or null on success.
  const applyPastedToml = (content: string): string | null => {
    const items = extractStartArrayItems(content);
    if (!items) {
      return 'No start = [...] array found in the pasted TOML';
    }
    if (items.length === 0) {
      return 'The start array in the pasted TOML is empty';
    }

    let instanceIdCounter = nextInstanceId;
    const agentsWithIds: OcfAgentWithMetadata[] = items.map((line, index) => {
      const parsed = parseOcfLine(line);
      if (parsed) {
        return {
          position: {
            section: 'resources',
            array_index: null,
            key: 'start',
            index,
          },
          item: {
            original: line,
            is_ocf: true,
            ocf_agent: {
              original: line,
              provider: parsed.provider,
              agent_type: parsed.agent_type,
              instance_name: parsed.instance_name,
              params: parsed.params,
            },
          },
          metadata: null,
          instanceId: instanceIdCounter++,
        } as any;
      }
      // Plain systemd unit / mount
      return {
        position: {
          section: 'resources',
          array_index: null,
          key: 'start',
          index,
        },
        item: { original: line, is_ocf: false, ocf_agent: null },
        metadata: null,
        instanceId: instanceIdCounter++,
      } as any;
    });

    setParsedAgents(agentsWithIds);
    setNextInstanceId(instanceIdCounter);
    setAddedParams(new Map());
    setExpandedKeys(new Set());

    // Rebuild form data from the imported agents
    const newFormAgents = agentsWithIds.map((agentWithMeta) => {
      if (agentWithMeta.item.is_ocf && agentWithMeta.item.ocf_agent) {
        return {
          params: paramsToRecord(agentWithMeta.item.ocf_agent.params || []),
          original: agentWithMeta.item.original,
        };
      }
      return { original: agentWithMeta.item.original };
    });
    form.setFieldValue('agents', newFormAgents);

    markDirty();
    setPasteTomlVisible(false);
    toast.success(`Imported ${agentsWithIds.length} entries from pasted TOML`);
    return null;
  };

  // 切换展开状态
  const toggleExpand = (key: string) => {
    const newExpanded = new Set(expandedKeys);

    if (newExpanded.has(key)) {
      newExpanded.delete(key);
    } else {
      newExpanded.add(key);
    }

    setExpandedKeys(newExpanded);
  };

  // Sync a single field change to parsedAgents immediately.
  // Replaces the old Form `onValuesChange`: the controlled field already wrote
  // to the form via setFieldValue, here we mirror the change into parsedAgents.
  const handleFieldChange = (
    idx: number,
    field: 'params' | 'original',
  ) => {
    if (idx >= 0) {
      const agent = parsedAgents[idx];
      const changedValue = form.getFieldValue(['agents', idx]);

      if (agent && changedValue) {
        const newAgent = { ...agent };

        // For OCF agents with params
        if (
          field === 'params' &&
          agent.item.is_ocf &&
          agent.item.ocf_agent &&
          changedValue.params
        ) {
          // Merge params: convert ParamEntry[] to Record, merge, then convert back to ParamEntry[]
          const originalParamsRecord = paramsToRecord(
            agent.item.ocf_agent.params || [],
          );
          const mergedParamsRecord = {
            ...originalParamsRecord,
            ...changedValue.params,
          };

          // Convert merged Record back to ParamEntry[], preserving order from original
          // For keys that exist in original, keep their order
          // For new keys from changedValue, append them
          const mergedParams: ParamEntry[] = [];

          // First, add all original params (with potentially updated values)
          for (const entry of agent.item.ocf_agent.params || []) {
            const key = entry.key;
            // Use updated value if exists, otherwise use original
            const value = mergedParamsRecord[key] ?? entry.value;
            mergedParams.push({ key, value });
            // Mark as processed
            delete mergedParamsRecord[key];
          }

          // Then add any new params from changedValue
          for (const [key, value] of Object.entries(mergedParamsRecord)) {
            mergedParams.push({ key, value: value as string });
          }

          // Update ocf_agent
          newAgent.item = {
            ...agent.item,
            ocf_agent: {
              ...agent.item.ocf_agent,
              params: mergedParams,
            },
          };

          // CRITICAL: Also update the 'original' field to match the new params
          // Regenerate the OCF string with updated params
          const ocfAgent = agent.item.ocf_agent;
          newAgent.item.original = `ocf:${ocfAgent.provider}:${ocfAgent.agent_type} ${ocfAgent.instance_name}`;

          // Add parameters to the original string (using mergedParamsRecord)
          const paramStr = mergedParams
            .filter(({ value }) => value !== undefined && value !== '')
            .map(({ key, value }) => {
              if (
                String(value).includes(' ') ||
                String(value).includes(',') ||
                String(value) === ''
              ) {
                return `${key}='${value}'`;
              }
              return `${key}=${value}`;
            })
            .join(' ');

          if (paramStr) {
            newAgent.item.original += ` ${paramStr}`;
          }

          // Also update the ocf_agent.original field (non-null: rebuilt above)
          newAgent.item.ocf_agent = {
            ...(newAgent.item.ocf_agent as ParsedOcfAgent),
            original: newAgent.item.original,
          };
        }
        // For systemd units
        else if (
          field === 'original' &&
          !agent.item.is_ocf &&
          changedValue.original !== undefined
        ) {
          newAgent.item = {
            ...agent.item,
            original: changedValue.original,
          };
        }

        // Only update the specific agent that changed
        const newAgents = [...parsedAgents];
        newAgents[idx] = newAgent;
        setParsedAgents(newAgents);
        markDirty();
      }
    }

    // Trigger preview update
    forceUpdate({});

    // Sync to parent form in create mode
    if (mode === 'create') {
      syncToParentForm();
    }
  };

  // 删除 agent
  const deleteAgent = (index: number) => {
    const newAgents = parsedAgents.filter((_, i) => i !== index);

    // DO NOT update position.index - keep original value for stable tracking
    // No need to rebuild the array, just filter

    // Update form
    const currentValues = form.getFieldsValue();
    const newAgentsData = (currentValues.agents || []).filter(
      (_: any, i: number) => i !== index,
    );
    form.setFieldValue('agents', newAgentsData);

    // No need to update addedParams - it uses stable keys (position.index)
    // The deleted agent's params will be unused, but that's fine

    setParsedAgents(newAgents);
    markDirty();
    toast.success('Agent removed');
  };

  // 删除参数
  // stableKey parameter is the instanceId
  const handleRemoveParam = (stableKey: number, paramName: string) => {
    // Find agent by instanceId
    const arrayIndex = parsedAgents.findIndex(
      (a: any) => a.instanceId === stableKey,
    );
    if (arrayIndex === -1) return;

    const agent = parsedAgents[arrayIndex];
    if (!agent.item.ocf_agent) return;

    // Remove from params (filter out the param to remove)
    const newParams = (agent.item.ocf_agent.params || []).filter(
      (p: ParamEntry) => p.key !== paramName,
    );

    // Update agent
    const newAgent = {
      ...agent,
      item: {
        ...agent.item,
        ocf_agent: {
          ...agent.item.ocf_agent,
          params: newParams,
        },
      },
    };

    const newAgents = [...parsedAgents];
    newAgents[arrayIndex] = newAgent;
    setParsedAgents(newAgents);

    // Update form
    const currentValues = form.getFieldsValue();
    if (currentValues.agents?.[arrayIndex]?.params) {
      const newFormParams = { ...currentValues.agents[arrayIndex].params };
      delete newFormParams[paramName];
      form.setFieldValue(['agents', arrayIndex, 'params'], newFormParams);
    }

    // Remove from addedParams tracking
    setAddedParams((prev) => {
      const newMap = new Map(prev);
      const params = newMap.get(stableKey);
      if (params) {
        params.delete(paramName);
        if (params.size === 0) {
          newMap.delete(stableKey);
        } else {
          newMap.set(stableKey, params);
        }
      }
      return newMap;
    });

    markDirty();
    toast.success(`Parameter ${paramName} removed`);
  };

  // 打开添加参数Modal
  // stableKey parameter is the instanceId
  const handleAddParam = (stableKey: number) => {
    setCurrentAgentIndex(stableKey);
    setSelectedParam('');
    setAddParamModalVisible(true);
  };

  // 确认添加参数
  const confirmAddParam = () => {
    if (currentAgentIndex === null || !selectedParam) {
      toast.error('Please select a parameter');
      return;
    }

    const stableKey = currentAgentIndex;
    // Find agent by instanceId
    const arrayIndex = parsedAgents.findIndex(
      (a: any) => a.instanceId === stableKey,
    );
    if (arrayIndex === -1) return;

    const agent = parsedAgents[arrayIndex];
    if (!agent.item.ocf_agent || !agent.metadata) return;

    // Find parameter metadata
    const paramMeta = agent.metadata.parameters.find(
      (p) => p.name === selectedParam,
    );
    if (!paramMeta) return;

    // Add to params with default value (append new param to end)
    const newParamEntry: ParamEntry = {
      key: selectedParam,
      value: paramMeta.default || '',
    };
    const newParams = [...(agent.item.ocf_agent.params || []), newParamEntry];

    // Update agent
    const newAgent = {
      ...agent,
      item: {
        ...agent.item,
        ocf_agent: {
          ...agent.item.ocf_agent,
          params: newParams,
        },
      },
    };

    const newAgents = [...parsedAgents];
    newAgents[arrayIndex] = newAgent;
    setParsedAgents(newAgents);

    // Update form
    const currentValues = form.getFieldsValue();
    const params = currentValues.agents?.[arrayIndex]?.params || {};
    form.setFieldValue(['agents', arrayIndex, 'params'], {
      ...params,
      [selectedParam]: paramMeta.default || '',
    });

    // Add to addedParams tracking using stable key
    setAddedParams((prev) => {
      const newMap = new Map(prev);
      const existing = newMap.get(stableKey) || new Set();
      existing.add(selectedParam);
      newMap.set(stableKey, existing);
      return newMap;
    });

    markDirty();
    toast.success(`Parameter ${selectedParam} added`);
    setAddParamModalVisible(false);
    setSelectedParam('');
    setCurrentAgentIndex(null);
  };

  // 添加新 agent
  const handleAddAgent = () => {
    if (addAgentType === 'systemd') {
      if (!systemdUnit.trim()) {
        toast.error('Please enter systemd unit name');
        return;
      }

      const instanceId = nextInstanceId;

      const newAgent: OcfAgentWithMetadata = {
        position: {
          section: 'resources',
          array_index: null,
          key: 'start',
          index: parsedAgents.length, // This is for TOML output, not tracking
        },
        item: {
          original: systemdUnit.trim(),
          is_ocf: false,
          ocf_agent: null,
        },
        metadata: null,
        instanceId, // Unique instance ID for tracking
      } as any;

      const newAgents = [...parsedAgents, newAgent];
      setParsedAgents(newAgents);

      // Increment instance ID counter
      setNextInstanceId(nextInstanceId + 1);

      // Update form
      const currentValues = form.getFieldsValue();
      const agents = currentValues.agents || [];
      form.setFieldValue('agents', [
        ...agents,
        { original: systemdUnit.trim() },
      ]);

      markDirty();
      toast.success(`Added systemd unit: ${systemdUnit.trim()}`);
      closeAddModal();
    } else if (addAgentType === 'ocf') {
      if (!selectedProvider || !selectedAgent) {
        toast.error('Please select provider and agent');
        return;
      }

      // Find metadata for selected agent
      const providerAgents = allAgents?.providers[selectedProvider] || [];
      const agentMetadata = providerAgents.find(
        (a) => a.name === selectedAgent,
      );

      if (!agentMetadata) {
        toast.error('Agent metadata not found');
        return;
      }

      // Generate default params from metadata
      const defaultParamsList: ParamEntry[] = [];
      const defaultParamsRecord: Record<string, string> = {};

      agentMetadata.parameters.forEach((param) => {
        // Only include required params
        if (param.required) {
          const value = param.default || '';
          defaultParamsList.push({ key: param.name, value });
          defaultParamsRecord[param.name] = value;
        }
      });

      const instanceName = `${selectedAgent}_new`;

      // Use nextInstanceId as the unique instance ID
      const instanceId = nextInstanceId;

      const newAgent: OcfAgentWithMetadata = {
        position: {
          section: 'resources',
          array_index: null,
          key: 'start',
          index: parsedAgents.length, // This is for TOML output, not tracking
        },
        item: {
          original: `ocf:${selectedProvider}:${selectedAgent} ${instanceName}`,
          is_ocf: true,
          ocf_agent: {
            original: `ocf:${selectedProvider}:${selectedAgent} ${instanceName}`,
            provider: selectedProvider,
            agent_type: selectedAgent,
            instance_name: instanceName,
            params: defaultParamsList,
          },
        },
        metadata: agentMetadata,
        instanceId, // Unique instance ID for tracking
      } as any;

      const newAgents = [...parsedAgents, newAgent];
      setParsedAgents(newAgents);

      // Increment instance ID counter for next agent
      setNextInstanceId(nextInstanceId + 1);

      // Update form
      const currentValues = form.getFieldsValue();
      const agents = currentValues.agents || [];
      form.setFieldValue('agents', [
        ...agents,
        {
          original: `ocf:${selectedProvider}:${selectedAgent} ${instanceName}`,
          params: defaultParamsRecord,
        },
      ]);

      markDirty();
      toast.success(`Added OCF agent: ${selectedProvider}:${selectedAgent}`);
      closeAddModal();
    }
  };

  const openAddModal = () => {
    setAddModalVisible(true);
    setAddAgentType('ocf');
    setSelectedProvider('');
    setSelectedAgent('');
    setSystemdUnit('');
  };

  const closeAddModal = () => {
    setAddModalVisible(false);
  };

  // Generate IDs for DnD - use instanceId for stable keys
  const items = useMemo(
    () => parsedAgents.map((agent) => `agent-${agent.instanceId}`),
    [parsedAgents],
  );

  if (mode === 'edit' && !profile) {
    return (
      <div style={{ textAlign: 'center', padding: '40px' }}>
        <span className="text-muted-foreground">No profile selected</span>
      </div>
    );
  }

  return (
    <div
      className="ocf-agent-editor"
      style={{
        padding: mode === 'create' ? '0' : '24px',
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      {/* Header - only show in edit mode */}
      {mode === 'edit' && (
        <div
          className="flex items-center justify-between"
          style={{ marginBottom: '24px', flexShrink: 0 }}
        >
          <h3 className="m-0 text-xl font-semibold">
            OCF Agents Editor: {profile?.name}
          </h3>
          <div className="flex items-center gap-2">
            <Button variant="outline" onClick={loadParsedAgents}>
              <RotateCw className="mr-2 h-4 w-4" />
              Reload
            </Button>
            <Button onClick={handleSave} disabled={saving}>
              {saving ? (
                <Spinner className="mr-2 h-4 w-4" />
              ) : (
                <Save className="mr-2 h-4 w-4" />
              )}
              Save Changes
            </Button>
            {onCancel && (
              <Button variant="outline" onClick={onCancel}>
                Cancel
              </Button>
            )}
          </div>
        </div>
      )}

      {/* Stats */}
      <div
        className="flex flex-wrap items-center gap-2"
        style={{ marginBottom: '16px', flexShrink: 0 }}
      >
        <span className="text-sm font-semibold">Total OCF Agents:</span>
        <Badge variant="secondary">{parsedAgents.length}</Badge>

        {allAgents ? (
          <>
            <span className="ml-4 text-sm font-semibold">
              Available Providers:
            </span>
            {Object.keys(allAgents.providers)
              .sort()
              .map((provider) => (
                <Badge key={provider} variant="outline">
                  {provider}
                </Badge>
              ))}
          </>
        ) : (
          <>
            <span className="ml-4 text-sm font-semibold">
              Loading OCF agent parameters...
            </span>
            <Spinner className="h-4 w-4" />
          </>
        )}
      </div>

      {/* Main Content: Split View */}
      <div
        ref={containerRef}
        style={{
          display: 'flex',
          flex: 1,
          overflow: 'hidden',
          alignItems: 'stretch',
        }}
      >
        {/* Left Panel - Editor */}
        <div
          style={{
            flex: previewVisible ? `0 0 ${leftPanelWidth}%` : '1 1 100%',
            minWidth: previewVisible ? '20%' : 'auto',
            overflow: 'hidden',
          }}
        >
          <div className="flex h-full flex-col rounded-lg border border-border bg-card">
            {/* Card title */}
            <div className="flex items-center justify-between border-b border-border px-4 py-3">
              <div className="flex items-center gap-2">
                {mode === 'edit' && (
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7"
                    onClick={togglePreview}
                    title={previewVisible ? 'Hide preview' : 'Show preview'}
                  >
                    {previewVisible ? (
                      <Eye className="h-4 w-4" />
                    ) : (
                      <EyeOff className="h-4 w-4" />
                    )}
                  </Button>
                )}
                <span className="text-sm font-semibold">Editor</span>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setPasteTomlVisible(true)}
                  title="Import start array from pasted TOML"
                >
                  <ClipboardPaste className="mr-1 h-4 w-4" />
                  Paste TOML
                </Button>
                <Button size="sm" onClick={openAddModal}>
                  <Plus className="mr-1 h-4 w-4" />
                  Add Agent
                </Button>
              </div>
            </div>

            {/* Card body */}
            <div
              className="relative"
              style={{
                padding: '16px',
                height: 'calc(100% - 57px)',
                overflow: 'auto',
              }}
            >
              {loading && (
                <div className="mb-2 flex items-center gap-2 text-sm text-muted-foreground">
                  <Spinner className="h-4 w-4" />
                  Loading OCF agents...
                </div>
              )}

              {/* OCF Agents List with Drag and Drop */}
              {parsedAgents.length === 0 ? (
                <Empty description="No OCF agents found in start array" />
              ) : (
                <DndContext
                  sensors={sensors}
                  collisionDetection={closestCenter}
                  onDragEnd={handleDragEnd}
                >
                  <SortableContext
                    items={items}
                    strategy={verticalListSortingStrategy}
                  >
                    {parsedAgents.map((agentWithMeta, index) => {
                      const { item } = agentWithMeta;

                      // 尝试从 allAgents 中查找元数据 (只对 OCF agents)
                      const matchedMetadata =
                        item.is_ocf && item.ocf_agent
                          ? findAgentMetadata(item.ocf_agent)
                          : null;

                      // 如果后端已经返回了元数据，使用它；否则使用匹配的
                      const metadata =
                        agentWithMeta.metadata || matchedMetadata;
                      const isLoadingMetadata =
                        item.is_ocf && !metadata && !allAgents;

                      // Use instanceId for stable ID
                      const id = `agent-${agentWithMeta.instanceId}`;

                      return (
                        <SortableAgentItem
                          key={id}
                          id={id}
                          index={index}
                          agentWithMeta={agentWithMeta}
                          metadata={metadata}
                          isLoadingMetadata={isLoadingMetadata}
                          currentTheme={currentTheme}
                          form={form}
                          onFieldChange={handleFieldChange}
                          onDelete={deleteAgent}
                          onExpand={toggleExpand}
                          expandedKeys={expandedKeys}
                          onRemoveParam={handleRemoveParam}
                          onAddParam={handleAddParam}
                          addedParams={addedParams}
                        />
                      );
                    })}
                  </SortableContext>
                </DndContext>
              )}
            </div>
          </div>
        </div>

        {/* Drag Handle - only shown when preview is visible */}
        {previewVisible && (
          <div
            onMouseDown={handleMouseDown}
            title="Drag to resize"
            style={{
              cursor: 'col-resize',
              width: 20,
              userSelect: 'none',
              flex: '0 0 auto',
              alignSelf: 'stretch',
              display: 'flex',
              alignItems: 'flex-start',
              justifyContent: 'center',
              paddingTop: 16,
            }}
          >
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 4,
              }}
            >
              <div
                style={{
                  width: 3,
                  height: 3,
                  borderRadius: '50%',
                  background: '#bfbfbf',
                }}
              />
              <div
                style={{
                  width: 3,
                  height: 3,
                  borderRadius: '50%',
                  background: '#bfbfbf',
                }}
              />
              <div
                style={{
                  width: 3,
                  height: 3,
                  borderRadius: '50%',
                  background: '#bfbfbf',
                }}
              />
            </div>
          </div>
        )}

        {/* Right Panel - Live Preview */}
        {previewVisible && (
          <div
            style={{
              flex: `0 0 ${100 - leftPanelWidth}%`,
              minWidth: '20%',
              overflow: 'hidden',
            }}
          >
            <AgentPreview
              parsedAgents={parsedAgents}
              loading={loading}
              currentTheme={currentTheme}
            />
          </div>
        )}
      </div>

      {/* Add Agent Modal */}
      <AddAgentModal
        visible={addModalVisible}
        onOk={handleAddAgent}
        onCancel={closeAddModal}
        agentType={addAgentType}
        onAgentTypeChange={setAddAgentType}
        systemdUnit={systemdUnit}
        onSystemdUnitChange={setSystemdUnit}
        selectedProvider={selectedProvider}
        onProviderChange={setSelectedProvider}
        selectedAgent={selectedAgent}
        onAgentChange={setSelectedAgent}
        allAgents={allAgents}
        currentTheme={currentTheme}
      />

      {/* Add Parameter Modal */}
      <AddParameterModal
        visible={addParamModalVisible}
        onOk={confirmAddParam}
        onCancel={() => {
          setAddParamModalVisible(false);
          setSelectedParam('');
          setCurrentAgentIndex(null);
        }}
        currentAgentIndex={currentAgentIndex}
        selectedParam={selectedParam}
        onParamChange={setSelectedParam}
        parsedAgents={parsedAgents}
      />

      {/* Paste TOML Modal */}
      <PasteTomlModal
        visible={pasteTomlVisible}
        onCancel={() => setPasteTomlVisible(false)}
        onApply={applyPastedToml}
      />
    </div>
  );
}
