import {
  SaveOutlined,
  SyncOutlined,
  ReloadOutlined,
  DeleteOutlined,
  HolderOutlined,
  CaretRightOutlined,
  InfoCircleOutlined,
  QuestionCircleOutlined,
  PlusOutlined,
  MinusCircleOutlined,
} from '@ant-design/icons';
import {
  Button,
  Card,
  message,
  Space,
  Tag,
  Typography,
  Spin,
  Form,
  Popconfirm,
  Row,
  Col,
  Empty,
  Divider,
  Input,
  InputNumber,
  Switch,
  Tooltip,
  Modal,
  Select,
} from 'antd';
import { useEffect, useState, useRef } from 'react';
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  DragEndEvent,
} from '@dnd-kit/core';
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { haProfilesApi } from '@/api';
import type {
  OcfAgentWithMetadata,
  ResourceAgentsByProvider,
  ParsedOcfAgent,
  ResourceAgent,
} from '@/api/ha-profiles';
import { useThemeStore } from '@/stores/theme';

const { Title, Text } = Typography;

interface OcfAgentEditorProps {
  profile: { name: string; id: string } | null;
  onSave?: () => void;
  onCancel?: () => void;
}

// Helper function to generate OCF string from agent data
function generateOcfString(agent: ParsedOcfAgent, params?: Record<string, any>): string {
  const { provider, agent_type, instance_name } = agent;
  const finalParams = params || agent.params;

  // Build key=value pairs
  const paramStr = Object.entries(finalParams)
    .map(([key, value]) => {
      if (value === undefined || value === null) return '';
      // Quote values if they contain spaces or special characters
      if (String(value).includes(' ') || String(value).includes(',') || String(value) === '') {
        return `${key}='${value}'`;
      }
      return `${key}=${value}`;
    })
    .filter(Boolean)
    .join(' ');

  return `ocf:${provider}:${agent_type} ${instance_name} ${paramStr}`;
}

// Sortable Item Component
interface SortableItemProps {
  id: string;
  index: number;
  agentWithMeta: OcfAgentWithMetadata;
  metadata: ResourceAgent | null;
  isLoadingMetadata: boolean;
  currentTheme: string;
  onDelete: (index: number) => void;
  onExpand: (key: string) => void;
  expandedKeys: Set<string>;
  onRemoveParam: (index: number, paramName: string) => void;
  onAddParam: (index: number) => void;
  addedParams: Map<number, Set<string>>;
}

function SortableAgentItem({
  id,
  index,
  agentWithMeta,
  metadata,
  isLoadingMetadata,
  currentTheme,
  onDelete,
  onExpand,
  expandedKeys,
  onRemoveParam,
  onAddParam,
  addedParams,
}: SortableItemProps) {
  // Get instanceId for stable key lookup
  // Fallback to array index if instanceId not set
  const stableKey = (agentWithMeta as any).instanceId ?? index;
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  const { item, position } = agentWithMeta;
  const panelKey = `${position.section}-${position.key}-${index}`;
  const isExpanded = expandedKeys.has(panelKey);

  // Determine if this is an OCF agent or plain systemd unit
  const isOcf = item.is_ocf;
  const ocfAgent = item.ocf_agent;

  // Render form field for plain systemd unit
  const renderPlainUnitField = () => {
    const fieldName = ['agents', index, 'original'];

    return (
      <Form.Item
        name={fieldName}
        label="Systemd Unit"
        initialValue={item.original}
        rules={[
          {
            required: true,
            message: 'Unit name is required',
          },
        ]}
      >
        <Input placeholder="e.g., var-lib-linstor.mount" />
      </Form.Item>
    );
  };

  // Render form field based on parameter type (for OCF agents)
  const renderFormField = (param: any) => {
    if (!ocfAgent) return null;

    const fieldName = [`agents`, index, `params`, param.name];
    const currentValue = ocfAgent.params[param.name];

    switch (param.type) {
      case 'integer':
        return (
          <Form.Item
            key={param.name}
            name={fieldName}
            label={
              <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                <span>{param.name}</span>
                <Tooltip title={param.shortdesc || param.name}>
                  <InfoCircleOutlined style={{ color: '#999', fontSize: '12px' }} />
                </Tooltip>
              </div>
            }
            tooltip={
              param.longdesc
                ? {
                    title: (
                      <div style={{ whiteSpace: 'pre-wrap', maxHeight: '60px', overflow: 'auto', fontSize: '12px' }}>
                        {param.longdesc}
                      </div>
                    ),
                    icon: <QuestionCircleOutlined />,
                  }
                : undefined
            }
            initialValue={currentValue}
            rules={[
              {
                required: param.required,
                message: `${param.name} is required`,
              },
            ]}
          >
            <InputNumber style={{ width: '100%' }} />
          </Form.Item>
        );

      case 'boolean':
        return (
          <Form.Item
            key={param.name}
            name={fieldName}
            label={
              <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                <span>{param.name}</span>
                <Tooltip title={param.shortdesc || param.name}>
                  <InfoCircleOutlined style={{ color: '#999', fontSize: '12px' }} />
                </Tooltip>
              </div>
            }
            tooltip={
              param.longdesc
                ? {
                    title: (
                      <div style={{ whiteSpace: 'pre-wrap', maxHeight: '60px', overflow: 'auto', fontSize: '12px' }}>
                        {param.longdesc}
                      </div>
                    ),
                    icon: <QuestionCircleOutlined />,
                  }
                : undefined
            }
            valuePropName="checked"
            initialValue={
              typeof currentValue === 'boolean'
                ? currentValue
                : currentValue === 'true' || currentValue === '1' || currentValue === 'yes'
            }
            rules={[
              {
                required: param.required,
                message: `${param.name} is required`,
              },
            ]}
          >
            <Switch />
          </Form.Item>
        );

      default:
        return (
          <Form.Item
            key={param.name}
            name={fieldName}
            label={
              <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                <span>{param.name}</span>
                <Tooltip title={param.shortdesc || param.name}>
                  <InfoCircleOutlined style={{ color: '#999', fontSize: '12px' }} />
                </Tooltip>
              </div>
            }
            tooltip={
              param.longdesc
                ? {
                    title: (
                      <div style={{ whiteSpace: 'pre-wrap', maxHeight: '60px', overflow: 'auto', fontSize: '12px' }}>
                        {param.longdesc}
                      </div>
                    ),
                    icon: <QuestionCircleOutlined />,
                  }
                : undefined
            }
            initialValue={currentValue}
            rules={[
              {
                required: param.required,
                message: `${param.name} is required`,
              },
            ]}
          >
            <Input />
          </Form.Item>
        );
    }
  };

  // Get display info for the item
  const getItemDisplay = () => {
    if (isOcf && ocfAgent) {
      return {
        typeLabel: ocfAgent.agent_type,
        typeColor: 'purple' as const,
        instanceLabel: ocfAgent.instance_name,
        instanceColor: 'orange' as const,
      };
    } else {
      // Plain systemd unit
      const name = item.original.split(' ')[0]; // Take first part as name
      return {
        typeLabel: 'systemd',
        typeColor: 'blue' as const,
        instanceLabel: name,
        instanceColor: 'green' as const,
      };
    }
  };

  const displayInfo = getItemDisplay();

  return (
    <div ref={setNodeRef} style={style} className="sortable-agent-item">
      <Card
        size="small"
        style={{
          background: currentTheme === 'dark' ? '#1e293b' : '#f8fafc',
          marginBottom: '8px',
          borderRadius: '8px',
          border: `1px solid ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
          cursor: 'grab',
        }}
        bodyStyle={{ padding: 0 }}
      >
        {/* Header - always visible */}
        <div
          style={{
            padding: '12px 16px',
            display: 'flex',
            alignItems: 'center',
            gap: '12px',
            cursor: 'pointer',
          }}
          onClick={() => onExpand(panelKey)}
          {...attributes}
        >
          {/* Drag handle - only this element is draggable */}
          <div
            {...listeners}
            style={{ cursor: 'grab', display: 'flex', alignItems: 'center' }}
            onClick={(e) => e.stopPropagation()}
          >
            <HolderOutlined
              style={{
                color: '#999',
                fontSize: '16px',
              }}
            />
          </div>

          {/* Expand/Collapse icon */}
          <CaretRightOutlined
            style={{
              color: '#999',
              fontSize: '12px',
              transition: 'transform 0.2s',
              transform: isExpanded ? 'rotate(90deg)' : 'rotate(0deg)',
            }}
          />

          {/* Item info */}
          <Space size="small" style={{ flex: 1 }}>
            <Tag color="blue">#{index + 1}</Tag>
            {isOcf && <Tag color="cyan">OCF</Tag>}
            <Tag color={displayInfo.typeColor}>{displayInfo.typeLabel}</Tag>
            {isOcf && ocfAgent && <Tag color={displayInfo.instanceColor}>{displayInfo.instanceLabel}</Tag>}
            {!isOcf && <Tag color={displayInfo.instanceColor}>{displayInfo.instanceLabel}</Tag>}
            {isOcf && isLoadingMetadata && <Tag color="processing">Loading metadata...</Tag>}
            {isOcf && !metadata && !isLoadingMetadata && <Tag color="warning">Metadata not found</Tag>}
          </Space>

          {/* Delete button */}
          <Popconfirm
            title="Delete this item?"
            onConfirm={(e) => {
              e?.stopPropagation();
              onDelete(index);
            }}
            okText="Yes"
            cancelText="No"
          >
            <Button
              type="text"
              size="small"
              danger
              icon={<DeleteOutlined />}
              onClick={(e) => e.stopPropagation()}
            />
          </Popconfirm>
        </div>

        {/* Expanded content */}
        <div
          style={{
            padding: '16px',
            borderTop: `1px solid ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
            display: isExpanded ? 'block' : 'none',
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {/* Original string (for reference) */}
          <div style={{ marginBottom: '16px' }}>
            <Text type="secondary">Original:</Text>
            <div
              style={{
                marginTop: '4px',
                padding: '8px',
                background: currentTheme === 'dark' ? '#0f172a' : '#f1f5f9',
                borderRadius: '4px',
                fontFamily: 'monospace',
                fontSize: '12px',
                wordBreak: 'break-all',
              }}
            >
              {item.original}
            </div>
          </div>

          {/* Plain systemd unit - simple input */}
          {!isOcf && (
            <div>
              <Text strong>Systemd Unit Configuration</Text>
              {renderPlainUnitField()}
            </div>
          )}

          {/* OCF Agent with metadata */}
          {isOcf && metadata && (
            <div style={{ marginBottom: '16px' }}>
              <Text strong style={{ fontSize: '14px' }}>
                {metadata.name}
              </Text>
              {metadata.shortdesc && (
                <div style={{ color: '#666', fontSize: '12px', marginTop: '4px' }}>
                  {metadata.shortdesc}
                </div>
              )}
            </div>
          )}

          {/* Form fields with metadata */}
          {isOcf && metadata && (
            <div style={{ marginBottom: '16px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '12px' }}>
                <Text strong>Parameters</Text>
                <Button
                  size="small"
                  icon={<PlusOutlined />}
                  onClick={() => onAddParam && onAddParam(stableKey)}
                >
                  Add Parameter
                </Button>
              </div>

              {/* Only show parameters that exist in TOML or were added by user */}
              {Object.keys(ocfAgent?.params || {}).map((paramName) => {
                const param = metadata.parameters.find(p => p.name === paramName);
                if (!param) return null;

                return (
                  <div key={paramName} style={{ position: 'relative', paddingRight: '40px' }}>
                    {renderFormField(param)}
                    <Button
                      type="text"
                      size="small"
                      danger
                      icon={<MinusCircleOutlined />}
                      onClick={() => onRemoveParam(stableKey, paramName)}
                      style={{
                        position: 'absolute',
                        right: '0',
                        top: param.type === 'boolean' ? '0' : '32px',
                      }}
                    />
                  </div>
                );
              })}

              {/* Show manually added parameters */}
              {Array.from(addedParams.get(stableKey) || []).map((paramName) => {
                const param = metadata.parameters.find(p => p.name === paramName);
                if (!param || ocfAgent?.params?.[paramName] !== undefined) return null;

                return (
                  <div key={paramName} style={{ position: 'relative', paddingRight: '40px' }}>
                    {renderFormField(param)}
                    <Button
                      type="text"
                      size="small"
                      danger
                      icon={<MinusCircleOutlined />}
                      onClick={() => onRemoveParam(stableKey, paramName)}
                      style={{
                        position: 'absolute',
                        right: '0',
                        top: param.type === 'boolean' ? '0' : '32px',
                      }}
                    />
                  </div>
                );
              })}
            </div>
          )}

          {/* OCF Agent without metadata */}
          {isOcf && !metadata && ocfAgent && (
            <div>
              <Text strong>Parsed Parameters:</Text>
              <div
                style={{
                  marginTop: '8px',
                  padding: '12px',
                  background: currentTheme === 'dark' ? '#0f172a' : '#f1f5f9',
                  borderRadius: '4px',
                }}
              >
                <pre style={{ margin: 0, fontSize: '12px' }}>
                  {JSON.stringify(ocfAgent.params, null, 2)}
                </pre>
              </div>
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}

export function OcfAgentEditor({ profile, onSave, onCancel }: OcfAgentEditorProps) {
  const { theme: currentTheme } = useThemeStore();
  const [form] = Form.useForm();

  const loadingRef = useRef(false);
  const [saving, setSaving] = useState(false);

  // 解析的 OCF agents（来自 start 数组）
  const [parsedAgents, setParsedAgents] = useState<OcfAgentWithMetadata[]>([]);

  // Original TOML content and resource name
  const [originalToml, setOriginalToml] = useState<string>('');
  const [resourceName, setResourceName] = useState<string>('');

  // Form values for live preview
  const [, forceUpdate] = useState({});

  // 所有可用的 resource agents（按 provider 分组）
  const [allAgents, setAllAgents] = useState<ResourceAgentsByProvider | null>(null);

  // 展开的 agent keys
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());

  // Add agent modal state
  const [addModalVisible, setAddModalVisible] = useState(false);
  const [addAgentType, setAddAgentType] = useState<'ocf' | 'systemd'>('ocf');
  const [selectedProvider, setSelectedProvider] = useState<string>('');
  const [selectedAgent, setSelectedAgent] = useState<string>('');
  const [systemdUnit, setSystemdUnit] = useState<string>('');

  // Track manually added parameters for each agent
  // Key is a unique instance ID that never changes
  const [addedParams, setAddedParams] = useState<Map<number, Set<string>>>(new Map());

  // Counter for generating unique instance IDs for agents
  const [nextInstanceId, setNextInstanceId] = useState(0);

  // Add parameter modal state
  const [addParamModalVisible, setAddParamModalVisible] = useState(false);
  const [currentAgentIndex, setCurrentAgentIndex] = useState<number | null>(null);
  const [selectedParam, setSelectedParam] = useState<string>('');

  // DnD sensors
  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  const loadParsedAgents = async () => {
    if (!profile) return;

    // 防止重复调用（React.StrictMode会导致effect执行两次）
    if (loadingRef.current) return;
    loadingRef.current = true;

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
        agent => agent.position.key === 'start'
      );

      console.log('Loaded agents from backend:', startAgents);
      console.log('Original TOML:', tomlResult.content);

      // Assign unique instance IDs to all agents (stored as a hidden property)
      let instanceIdCounter = 0;
      const agentsWithIds = startAgents.map(agent => ({
        ...agent,
        instanceId: instanceIdCounter++,  // Add instance ID property
      }));

      setParsedAgents(agentsWithIds);
      setNextInstanceId(instanceIdCounter);  // Next new agent will use this

      // Initialize form with agent data (use agentsWithIds, not startAgents)
      const initialValues = {
        agents: agentsWithIds.map(agentWithMeta => {
          if (agentWithMeta.item.is_ocf && agentWithMeta.item.ocf_agent) {
            return {
              params: agentWithMeta.item.ocf_agent.params,
              original: agentWithMeta.item.original,
            };
          } else {
            return {
              original: agentWithMeta.item.original,
            };
          }
        }),
      };

      console.log('Setting form initial values:', initialValues);

      // Wait for React to render the Form.Item components before setting values
      setTimeout(() => {
        form.setFieldsValue(initialValues);

        // Verify the form values were set correctly
        setTimeout(() => {
          const currentValues = form.getFieldsValue();
          console.log('Form values after setting:', currentValues);
          console.log('Form agents array:', currentValues.agents);
        }, 50);
      }, 50);
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      loadingRef.current = false;
    }
  };

  const loadAllResourceAgents = async () => {
    try {
      const result = await haProfilesApi.getAllResourceAgents();
      setAllAgents(result);
    } catch (err) {
      console.error('Failed to load all resource agents:', err);
      message.warning('Failed to load agent metadata');
    }
  };

  // 加载 TOML 并解析 OCF agents
  useEffect(() => {
    if (profile) {
      loadParsedAgents();
    }
  }, [profile]);

  // 加载所有可用的 resource agents（异步，只加载一次）
  useEffect(() => {
    if (parsedAgents.length > 0 && !allAgents) {
      loadAllResourceAgents();
    }
  }, [parsedAgents, allAgents]);

  const handleSave = async () => {
    if (!profile) return;

    setSaving(true);
    try {
      // Get current form values (with fallback)
      let values = form.getFieldsValue();

      // Try to validate, but use current values if validation fails
      try {
        values = await form.validateFields();
      } catch (validationError) {
        console.warn('Form validation failed, using current values:', validationError);
      }

      // Debug logging
      console.log('Form values on save:', values);
      console.log('parsedAgents:', parsedAgents);

      // Check if agents array exists
      if (!values.agents || !Array.isArray(values.agents)) {
        console.error('values.agents is missing or not an array:', values);
        message.error('Form data is invalid. Please try reloading the page.');
        return;
      }

      // Generate the updated start array strings
      const startArrayItems = values.agents.map((agentData: any, index: number) => {
        const item = parsedAgents[index].item;

        if (item.is_ocf && item.ocf_agent) {
          // OCF agent - generate string from form data
          const params = agentData.params || item.ocf_agent.params;
          const agent = item.ocf_agent;

          // Generate parameter string
          const paramStr = Object.entries(params)
            .filter(([_, value]) => value !== undefined && value !== '')
            .map(([key, value]) => {
              if (value.includes(' ') || value.includes(',') || value === '') {
                return `${key}='${value}'`;
              }
              return `${key}=${value}`;
            })
            .join(' ');

          return `ocf:${agent.provider}:${agent.agent_type} ${agent.instance_name}${paramStr ? ' ' + paramStr : ''}`;
        } else {
          // Non-OCF item - use original value from form
          return agentData.original || item.original;
        }
      });

      // Step 1: Update start array using the new API (automatically syncs and reloads drbd-reactor)
      message.loading('Saving and syncing configuration...');
      const result = await haProfilesApi.updateStartArray(profile.name, startArrayItems);

      if (result.success) {
        message.success(result.message || `Configuration saved and synced to ${result.synced_nodes.length} node(s)`);
      } else {
        message.warning(result.message || 'Configuration saved but sync had issues');
      }

      // Reload the parsed agents to reflect changes
      await loadParsedAgents();

      onSave?.();
    } catch (err) {
      console.error('Save failed:', err);
      message.error(`Failed to save: ${(err as { message: string }).message}`);
    } finally {
      setSaving(false);
    }
  };

  // 根据解析的 agent 查找匹配的元数据
  const findAgentMetadata = (agent: ParsedOcfAgent) => {
    if (!allAgents) return null;

    const providerAgents = allAgents.providers[agent.provider];
    if (!providerAgents) return null;

    return providerAgents.find(a => a.name === agent.agent_type) || null;
  };

  // 拖拽结束处理
  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;

    if (over && active.id !== over.id) {
      // Extract array indices from IDs
      const oldIndex = parseInt(String(active.id).replace('agent-', ''));
      const newIndex = parseInt(String(over.id).replace('agent-', ''));

      if (oldIndex >= 0 && newIndex >= 0) {
        const newAgents = arrayMove(parsedAgents, oldIndex, newIndex);

        // Update form order - preserve all agent data including params
        const currentValues = form.getFieldsValue();
        const newAgentsData = arrayMove(currentValues.agents || [], oldIndex, newIndex);
        form.setFieldValue('agents', newAgentsData);

        setParsedAgents(newAgents);
      }
    }
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

  // 删除 agent
  const deleteAgent = (index: number) => {
    const newAgents = parsedAgents.filter((_, i) => i !== index);

    // DO NOT update position.index - keep original value for stable tracking
    // No need to rebuild the array, just filter

    // Update form
    const currentValues = form.getFieldsValue();
    const newAgentsData = (currentValues.agents || []).filter((_: any, i: number) => i !== index);
    form.setFieldValue('agents', newAgentsData);

    // No need to update addedParams - it uses stable keys (position.index)
    // The deleted agent's params will be unused, but that's fine

    setParsedAgents(newAgents);
    message.success('Agent removed');
  };

  // 删除参数
  // stableKey parameter is the instanceId
  const handleRemoveParam = (stableKey: number, paramName: string) => {
    // Find agent by instanceId
    const arrayIndex = parsedAgents.findIndex((a: any) => a.instanceId === stableKey);
    if (arrayIndex === -1) return;

    const agent = parsedAgents[arrayIndex];
    if (!agent.item.ocf_agent) return;

    // Remove from params
    const newParams = { ...agent.item.ocf_agent.params };
    delete newParams[paramName];

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
    setAddedParams(prev => {
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

    message.success(`Parameter ${paramName} removed`);
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
      message.error('Please select a parameter');
      return;
    }

    const stableKey = currentAgentIndex;
    // Find agent by instanceId
    const arrayIndex = parsedAgents.findIndex((a: any) => a.instanceId === stableKey);
    if (arrayIndex === -1) return;

    const agent = parsedAgents[arrayIndex];
    if (!agent.item.ocf_agent || !agent.metadata) return;

    // Find parameter metadata
    const paramMeta = agent.metadata.parameters.find(p => p.name === selectedParam);
    if (!paramMeta) return;

    // Add to params with default value
    const newParams = { ...agent.item.ocf_agent.params };
    newParams[selectedParam] = paramMeta.default || '';

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
    setAddedParams(prev => {
      const newMap = new Map(prev);
      const existing = newMap.get(stableKey) || new Set();
      existing.add(selectedParam);
      newMap.set(stableKey, existing);
      return newMap;
    });

    message.success(`Parameter ${selectedParam} added`);
    setAddParamModalVisible(false);
    setSelectedParam('');
    setCurrentAgentIndex(null);
  };

  // 添加新 agent
  const handleAddAgent = () => {
    if (addAgentType === 'systemd') {
      if (!systemdUnit.trim()) {
        message.error('Please enter systemd unit name');
        return;
      }

      const instanceId = nextInstanceId;

      const newAgent: OcfAgentWithMetadata = {
        position: {
          section: 'resources',
          array_index: null,
          key: 'start',
          index: parsedAgents.length,  // This is for TOML output, not tracking
        },
        item: {
          original: systemdUnit.trim(),
          is_ocf: false,
          ocf_agent: null,
        },
        metadata: null,
        instanceId,  // Unique instance ID for tracking
      } as any;

      const newAgents = [...parsedAgents, newAgent];
      setParsedAgents(newAgents);

      // Increment instance ID counter
      setNextInstanceId(nextInstanceId + 1);

      // Update form
      const currentValues = form.getFieldsValue();
      const agents = currentValues.agents || [];
      form.setFieldValue('agents', [...agents, { original: systemdUnit.trim() }]);

      message.success(`Added systemd unit: ${systemdUnit.trim()}`);
      closeAddModal();
    } else if (addAgentType === 'ocf') {
      if (!selectedProvider || !selectedAgent) {
        message.error('Please select provider and agent');
        return;
      }

      // Find metadata for selected agent
      const providerAgents = allAgents?.providers[selectedProvider] || [];
      const agentMetadata = providerAgents.find(a => a.name === selectedAgent);

      if (!agentMetadata) {
        message.error('Agent metadata not found');
        return;
      }

      // Generate default params from metadata
      const defaultParams: Record<string, string> = {};
      agentMetadata.parameters.forEach(param => {
        // Include required params and params with defaults
        if (param.required || (param.default && param.default !== '')) {
          defaultParams[param.name] = param.default || '';
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
          index: parsedAgents.length,  // This is for TOML output, not tracking
        },
        item: {
          original: `ocf:${selectedProvider}:${selectedAgent} ${instanceName}`,
          is_ocf: true,
          ocf_agent: {
            original: `ocf:${selectedProvider}:${selectedAgent} ${instanceName}`,
            provider: selectedProvider,
            agent_type: selectedAgent,
            instance_name: instanceName,
            params: defaultParams,
          },
        },
        metadata: agentMetadata,
        instanceId,  // Unique instance ID for tracking
      } as any;

      const newAgents = [...parsedAgents, newAgent];
      setParsedAgents(newAgents);

      // Increment instance ID counter for next agent
      setNextInstanceId(nextInstanceId + 1);

      // Update form
      const currentValues = form.getFieldsValue();
      const agents = currentValues.agents || [];
      form.setFieldValue('agents', [...agents, {
        original: `ocf:${selectedProvider}:${selectedAgent} ${instanceName}`,
        params: defaultParams,
      }]);

      message.success(`Added OCF agent: ${selectedProvider}:${selectedAgent}`);
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

  if (!profile) {
    return (
      <div style={{ textAlign: 'center', padding: '40px' }}>
        <Text type="secondary">No profile selected</Text>
      </div>
    );
  }

  // Generate IDs for DnD - use array index for DnD Kit
  // DnD Kit needs items array to match the rendering order
  const items = parsedAgents.map((_, index) => `agent-${index}`);

  // Generate OCF string from agent data
  const generateAgentString = (itemWithMeta: OcfAgentWithMetadata, index: number): string => {
    // Directly use params from the item object, not from form
    // This avoids index mismatch issues after drag/reorder
    if (itemWithMeta.item.is_ocf && itemWithMeta.item.ocf_agent) {
      // OCF agent - use generateOcfString
      const agent = itemWithMeta.item.ocf_agent;
      const params = agent.params || {};

      return `    "${generateOcfString(agent, params)}"`;
    } else {
      // Plain systemd unit - just use the original value
      return `    "${itemWithMeta.item.original}"`;
    }
  };

  // Generate full TOML start array preview
  const generateTomlPreview = (): string => {
    const agentStrings = parsedAgents.map((agentWithMeta, index) => {
      return generateAgentString(agentWithMeta, index);
    });

    return `start = [\n${agentStrings.join(',\n')}\n  ]`;
  };

  return (
    <div className="ocf-agent-editor" style={{ padding: '24px', height: '100%', display: 'flex', flexDirection: 'column' }}>
      {/* Header */}
      <div style={{ marginBottom: '24px', flexShrink: 0 }}>
        <Row justify="space-between" align="middle">
          <Col>
            <Title level={3} style={{ margin: 0 }}>
              OCF Agents Editor: {profile.name}
            </Title>
          </Col>
          <Col>
            <Space>
              <Button
                icon={<ReloadOutlined />}
                onClick={loadParsedAgents}
              >
                Reload
              </Button>
              <Button
                type="primary"
                icon={<SaveOutlined />}
                onClick={handleSave}
                disabled={saving}
                loading={saving}
              >
                Save Changes
              </Button>
              {onCancel && <Button onClick={onCancel}>Cancel</Button>}
            </Space>
          </Col>
        </Row>
      </div>

      {/* Stats */}
      <div style={{ marginBottom: '16px', flexShrink: 0 }}>
        <Space>
          <Text strong>Total OCF Agents:</Text>
          <Tag color="blue">{parsedAgents.length}</Tag>

          {allAgents ? (
            <>
              <Text strong style={{ marginLeft: '16px' }}>Available Providers:</Text>
              {Object.keys(allAgents.providers).sort().map(provider => (
                <Tag key={provider} color="green">{provider}</Tag>
              ))}
            </>
          ) : (
            <>
              <Text strong style={{ marginLeft: '16px' }}>Loading OCF agent parameters...</Text>
              <Spin size="small" />
            </>
          )}
        </Space>
      </div>

      {/* Main Content: Split View */}
      <div style={{ display: 'flex', gap: '16px', flex: 1, overflow: 'hidden' }}>
        {/* Left Panel - Editor */}
        <div style={{ flex: 0.4, overflow: 'auto', minWidth: 0 }}>
          <Card
            title={
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <Text strong>Editor</Text>
                <Button
                  type="primary"
                  size="small"
                  icon={<PlusOutlined />}
                  onClick={openAddModal}
                >
                  Add Agent
                </Button>
              </div>
            }
            style={{ height: '100%' }}
            bodyStyle={{ padding: '16px', height: 'calc(100% - 57px)', overflow: 'auto' }}
          >
            {/* Main Form */}
            <Form
              form={form}
              layout="vertical"
              onFinish={handleSave}
              onValuesChange={() => forceUpdate({})}
              initialValues={{ agents: [] }}
            >
              {/* OCF Agents List with Drag and Drop */}
              {parsedAgents.length === 0 ? (
                <Empty description="No OCF agents found in start array" />
              ) : (
                <DndContext
                  sensors={sensors}
                  collisionDetection={closestCenter}
                  onDragEnd={handleDragEnd}
                >
                  <SortableContext items={items} strategy={verticalListSortingStrategy}>
                    {parsedAgents.map((agentWithMeta, index) => {
                      const { item, position } = agentWithMeta;

                      // 尝试从 allAgents 中查找元数据 (只对 OCF agents)
                      const matchedMetadata = item.is_ocf && item.ocf_agent
                        ? findAgentMetadata(item.ocf_agent)
                        : null;

                      // 如果后端已经返回了元数据，使用它；否则使用匹配的
                      const metadata = agentWithMeta.metadata || matchedMetadata;
                      const isLoadingMetadata = item.is_ocf && !metadata && !allAgents;

                      // Use simple agent-{index} ID to match items array
                      const id = `agent-${index}`;

                      return (
                        <SortableAgentItem
                          key={id}
                          id={id}
                          index={index}
                          agentWithMeta={agentWithMeta}
                          metadata={metadata}
                          isLoadingMetadata={isLoadingMetadata}
                          currentTheme={currentTheme}
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
            </Form>
          </Card>
        </div>

        {/* Right Panel - Live Preview */}
        <div style={{ flex: 0.6, overflow: 'hidden', minWidth: 0, display: 'flex', flexDirection: 'column' }}>
          <Card
            title={<Text strong>Live Preview (TOML)</Text>}
            style={{ height: '100%', display: 'flex', flexDirection: 'column' }}
            bodyStyle={{ padding: '16px', flex: 1, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}
          >
            <div
              style={{
                background: currentTheme === 'dark' ? '#0f172a' : '#f1f5f9',
                borderRadius: '8px',
                padding: '16px',
                fontFamily: 'monospace',
                fontSize: '13px',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word',
                border: `1px solid ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
                overflow: 'auto',
                flex: 1,
                maxHeight: '100%',
              }}
            >
              {generateTomlPreview()}
            </div>
          </Card>
        </div>
      </div>

      {/* Add Agent Modal */}
      <Modal
        title="Add New Agent"
        open={addModalVisible}
        onOk={handleAddAgent}
        onCancel={closeAddModal}
        width={600}
        okText="Add"
        cancelText="Cancel"
      >
        <Form layout="vertical" style={{ marginTop: '16px' }}>
          <Form.Item label="Agent Type">
            <Select
              value={addAgentType}
              onChange={setAddAgentType}
              options={[
                { label: 'OCF Agent', value: 'ocf' },
                { label: 'Systemd Unit', value: 'systemd' },
              ]}
            />
          </Form.Item>

          {addAgentType === 'systemd' && (
            <Form.Item
              label="Systemd Unit"
              help="e.g., nginx.service, var-lib-mysql.mount"
            >
              <Input
                placeholder="Enter systemd unit name"
                value={systemdUnit}
                onChange={(e) => setSystemdUnit(e.target.value)}
                onPressEnter={handleAddAgent}
              />
            </Form.Item>
          )}

          {addAgentType === 'ocf' && (
            <>
              <Form.Item label="Provider">
                <Select
                  placeholder="Select provider"
                  value={selectedProvider || undefined}
                  onChange={setSelectedProvider}
                  options={
                    allAgents
                      ? Object.keys(allAgents.providers).sort().map((p) => ({
                          label: p,
                          value: p,
                        }))
                      : []
                  }
                />
              </Form.Item>

              <Form.Item label="Agent">
                <Select
                  placeholder="Select agent"
                  value={selectedAgent || undefined}
                  onChange={setSelectedAgent}
                  disabled={!selectedProvider}
                  options={
                    selectedProvider && allAgents
                      ? allAgents.providers[selectedProvider]?.map((a) => ({
                          label: `${a.name} - ${a.shortdesc || ''}`,
                          value: a.name,
                        }))
                      : []
                  }
                  showSearch
                  filterOption={(input, option) =>
                    (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
                  }
                />
              </Form.Item>

              {selectedAgent && selectedProvider && allAgents && (
                <Form.Item label="Description">
                  <div
                    style={{
                      padding: '12px',
                      background: currentTheme === 'dark' ? '#1e293b' : '#f8fafc',
                      borderRadius: '4px',
                      fontSize: '13px',
                    }}
                  >
                    {
                      allAgents.providers[selectedProvider]?.find(
                        (a) => a.name === selectedAgent
                      )?.longdesc || 'No description available'
                    }
                  </div>
                </Form.Item>
              )}
            </>
          )}
        </Form>
      </Modal>

      {/* Add Parameter Modal */}
      {currentAgentIndex !== null && (
        <Modal
          title="Add Parameter"
          open={addParamModalVisible}
          onOk={confirmAddParam}
          onCancel={() => {
            setAddParamModalVisible(false);
            setSelectedParam('');
            setCurrentAgentIndex(null);
          }}
          width={600}
          okText="Add"
          cancelText="Cancel"
        >
          <Form layout="vertical" style={{ marginTop: '16px' }}>
            <Form.Item label="Parameter">
              <Select
                placeholder="Select parameter to add"
                value={selectedParam || undefined}
                onChange={setSelectedParam}
                showSearch
                filterOption={(input, option) =>
                  (option?.label ?? '').toLowerCase().includes(input.toLowerCase())
                }
                options={
                  currentAgentIndex !== null
                    ? (() => {
                        const agent = parsedAgents[currentAgentIndex];
                        if (!agent.metadata) return [];

                        const existingParams = new Set(
                          Object.keys(agent.item.ocf_agent?.params || {})
                        );

                        return agent.metadata.parameters
                          .filter(p => !existingParams.has(p.name))
                          .map(p => ({
                            label: `${p.name}${p.required ? ' (required)' : ''} - ${p.shortdesc || p.type}`,
                            value: p.name,
                          }));
                      })()
                    : []
                }
              />
            </Form.Item>

            {selectedParam && currentAgentIndex !== null && (
              <>
                <Form.Item label="Type">
                  <Tag color="blue">
                    {parsedAgents[currentAgentIndex].metadata?.parameters.find(
                      p => p.name === selectedParam
                    )?.type || 'unknown'}
                  </Tag>
                </Form.Item>

                <Form.Item label="Description">
                  <div
                    style={{
                      padding: '12px',
                      background: currentTheme === 'dark' ? '#1e293b' : '#f8fafc',
                      borderRadius: '4px',
                      fontSize: '13px',
                    }}
                  >
                    {
                      parsedAgents[currentAgentIndex].metadata?.parameters.find(
                        p => p.name === selectedParam
                      )?.longdesc || 'No description available'
                    }
                  </div>
                </Form.Item>

                <Form.Item label="Default Value">
                  <Text code>
                    {parsedAgents[currentAgentIndex].metadata?.parameters.find(
                      p => p.name === selectedParam
                    )?.default || '(empty)'}
                  </Text>
                </Form.Item>
              </>
            )}
          </Form>
        </Modal>
      )}
    </div>
  );
}
