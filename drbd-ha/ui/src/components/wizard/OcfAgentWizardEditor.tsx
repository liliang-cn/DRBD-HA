import {
  PlusOutlined,
  MinusCircleOutlined,
  HolderOutlined,
  CaretRightOutlined,
  InfoCircleOutlined,
  DeleteOutlined,
  QuestionCircleOutlined,
} from '@ant-design/icons';
import {
  Button,
  Card,
  Form,
  Input,
  InputNumber,
  Modal,
  message,
  Select,
  Space,
  Tag,
  Typography,
  Tooltip,
  Row,
  Col,
  Divider,
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
  ResourceAgentsByProvider,
  OcfAgentConfig,
  ResourceAgent,
  Parameter,
} from '@/types';
import { useThemeStore } from '@/stores/theme';

const { Text, Title } = Typography;

interface OcfAgentWizardEditorProps {
  form: any; // FormInstance
  resources: { name: string }[];
  services: string[];
}

interface OcfAgentItemData {
  type: 'ocf' | 'mount' | 'service';
  original: string;
  // For OCF agents
  provider?: string;
  agent_type?: string;
  instance_name?: string;
  params?: Record<string, string>;
  // For mount/service
  value?: string;
}

// Sortable Item Component
interface SortableItemProps {
  id: string;
  index: number;
  item: OcfAgentItemData;
  resources: { name: string }[];
  services: string[];
  currentTheme: string;
  allAgents: ResourceAgentsByProvider | null;
  onDelete: (index: number) => void;
  onExpand: (key: string) => void;
  expandedKeys: Set<string>;
  onRemoveParam: (index: number, paramName: string) => void;
  onAddParam: (index: number) => void;
  onUpdateItem: (index: number, data: OcfAgentItemData) => void;
  form: any;
}

function SortableAgentItem({
  id,
  index,
  item,
  resources,
  services,
  currentTheme,
  allAgents,
  onDelete,
  onExpand,
  expandedKeys,
  onRemoveParam,
  onAddParam,
  onUpdateItem,
  form,
}: SortableItemProps) {
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

  const panelKey = `agent-${index}`;
  const isExpanded = expandedKeys.has(panelKey);

  // Get metadata for OCF agents
  const metadata =
    item.type === 'ocf' && item.provider && item.agent_type && allAgents
      ? allAgents.providers[item.provider || '']?.find(
          (a) => a.name === item.agent_type
        ) || null
      : null;

  // Render form field based on parameter type
  const renderFormField = (param: Parameter) => {
    const fieldName = ['ocf_agents', index, 'params', param.name];
    const currentValue = item.params?.[param.name];

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
            <input type="checkbox" />
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

  const getItemDisplay = () => {
    if (item.type === 'ocf') {
      return {
        typeLabel: item.agent_type || 'Unknown',
        typeColor: 'purple' as const,
        instanceLabel: item.instance_name || '',
        instanceColor: 'orange' as const,
      };
    } else if (item.type === 'mount') {
      return {
        typeLabel: 'Mount',
        typeColor: 'blue' as const,
        instanceLabel: item.value || '',
        instanceColor: 'green' as const,
      };
    } else {
      return {
        typeLabel: 'Service',
        typeColor: 'cyan' as const,
        instanceLabel: item.value || '',
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
          {/* Drag handle */}
          <div
            {...listeners}
            style={{ cursor: 'grab', display: 'flex', alignItems: 'center' }}
            onClick={(e) => e.stopPropagation()}
          >
            <HolderOutlined style={{ color: '#999', fontSize: '16px' }} />
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
            {item.type === 'ocf' && <Tag color="cyan">OCF</Tag>}
            {item.type === 'mount' && <Tag color="blue">Mount</Tag>}
            {item.type === 'service' && <Tag color="green">Service</Tag>}
            <Tag color={displayInfo.typeColor}>{displayInfo.typeLabel}</Tag>
            <Tag color={displayInfo.instanceColor}>{displayInfo.instanceLabel}</Tag>
          </Space>

          {/* Delete button */}
          <Button
            type="text"
            size="small"
            danger
            icon={<DeleteOutlined />}
            onClick={(e) => {
              e.stopPropagation();
              onDelete(index);
            }}
          />
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
          {/* Mount Unit */}
          {item.type === 'mount' && (
            <Form.Item
              name={['ocf_agents', index, 'value']}
              label="Mount Unit"
              rules={[{ required: true, message: 'Mount unit is required' }]}
            >
              <Select
                placeholder="Select mount unit"
                options={resources.map((r) => ({
                  value: `${r.name}.mount`,
                  label: `${r.name}.mount`,
                }))}
              />
            </Form.Item>
          )}

          {/* Systemd Service */}
          {item.type === 'service' && (
            <Form.Item
              name={['ocf_agents', index, 'value']}
              label="Systemd Service"
              rules={[{ required: true, message: 'Service is required' }]}
            >
              <Select
                placeholder="Select service"
                showSearch
                options={services.map((s) => ({
                  value: s,
                  label: s,
                }))}
              />
            </Form.Item>
          )}

          {/* OCF Agent with metadata */}
          {item.type === 'ocf' && metadata && (
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
          {item.type === 'ocf' && metadata && (
            <div style={{ marginBottom: '16px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '12px' }}>
                <Text strong>Parameters</Text>
                <Button
                  size="small"
                  icon={<PlusOutlined />}
                  onClick={() => onAddParam(index)}
                >
                  Add Parameter
                </Button>
              </div>

              {Object.keys(item.params || {}).map((paramName) => {
                const param = metadata.parameters.find((p) => p.name === paramName);
                if (!param) return null;

                return (
                  <div key={paramName} style={{ position: 'relative', paddingRight: '40px' }}>
                    {renderFormField(param)}
                    <Button
                      type="text"
                      size="small"
                      danger
                      icon={<MinusCircleOutlined />}
                      onClick={() => onRemoveParam(index, paramName)}
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
        </div>
      </Card>
    </div>
  );
}

// Main component
export function OcfAgentWizardEditor({ form, resources, services }: OcfAgentWizardEditorProps) {
  const { theme: currentTheme } = useThemeStore();
  const [items, setItems] = useState<OcfAgentItemData[]>([]);
  const [allAgents, setAllAgents] = useState<ResourceAgentsByProvider | null>(null);

  // Expanded agent keys
  const [expandedKeys, setExpandedKeys] = useState<Set<string>>(new Set());

  // Add agent modal state
  const [addModalVisible, setAddModalVisible] = useState(false);
  const [addAgentType, setAddAgentType] = useState<'ocf' | 'mount' | 'service'>('ocf');
  const [selectedProvider, setSelectedProvider] = useState<string>('');
  const [selectedAgent, setSelectedAgent] = useState<string>('');

  // DnD sensors
  const sensors = useSensors(
    useSensor(PointerSensor),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  // Load form values
  useEffect(() => {
    const ocfAgents = form.getFieldValue('ocf_agents') || [];
    setItems(ocfAgents);
  }, [form]);

  // Load all resource agents
  const loadAllResourceAgents = async () => {
    try {
      const result = await haProfilesApi.getAllResourceAgents();
      setAllAgents(result);
    } catch (err) {
      console.error('Failed to load all resource agents:', err);
    }
  };

  useEffect(() => {
    if (items.length > 0 && !allAgents) {
      loadAllResourceAgents();
    }
  }, [items]);

  // Toggle expand state
  const toggleExpand = (key: string) => {
    const newExpanded = new Set(expandedKeys);
    if (newExpanded.has(key)) {
      newExpanded.delete(key);
    } else {
      newExpanded.add(key);
    }
    setExpandedKeys(newExpanded);
  };

  // Delete agent
  const deleteAgent = (index: number) => {
    const newItems = items.filter((_, i) => i !== index);
    setItems(newItems);
    const ocfAgents = form.getFieldValue('ocf_agents') || [];
    const newAgents = ocfAgents.filter((_: any, i: number) => i !== index);
    form.setFieldValue('ocf_agents', newAgents);
  };

  // Remove parameter
  const handleRemoveParam = (index: number, paramName: string) => {
    const item = items[index];
    if (!item.params) return;

    const newParams = { ...item.params };
    delete newParams[paramName];

    const newItem = { ...item, params: newParams };
    const newItems = [...items];
    newItems[index] = newItem;
    setItems(newItems);

    // Update form
    const ocfAgents = form.getFieldValue('ocf_agents') || [];
    if (ocfAgents[index]?.params) {
      const newFormParams = { ...ocfAgents[index].params };
      delete newFormParams[paramName];
      ocfAgents[index].params = newFormParams;
      form.setFieldValue('ocf_agents', ocfAgents);
    }
  };

  // Add parameter
  const handleAddParam = (index: number) => {
    const item = items[index];
    if (item.type !== 'ocf' || !allAgents) return;

    const metadata = allAgents.providers[item.provider || '']?.find(
      (a) => a.name === item.agent_type
    );

    if (!metadata) return;

    // Find first required or default parameter that's not set
    const existingParams = new Set(Object.keys(item.params || {}));
    const missingParam = metadata.parameters.find(
      (p) => !existingParams.has(p.name) && (p.required || (p.default && p.default !== ''))
    );

    if (missingParam) {
      const newParams = { ...(item.params || {}), [missingParam.name]: missingParam.default || '' };
      const newItem = { ...item, params: newParams };
      const newItems = [...items];
      newItems[index] = newItem;
      setItems(newItems);

      // Update form
      const ocfAgents = form.getFieldValue('ocf_agents') || [];
      ocfAgents[index].params = newParams;
      form.setFieldValue('ocf_agents', ocfAgents);
    }
  };

  // Drag end handler
  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;

    if (over && active.id !== over.id) {
      const oldIndex = items.findIndex((_, i) => `agent-${i}` === active.id);
      const newIndex = items.findIndex((_, i) => `agent-${i}` === over.id);

      if (oldIndex !== -1 && newIndex !== -1) {
        const newItems = arrayMove(items, oldIndex, newIndex);
        setItems(newItems);

        // Update form order
        const ocfAgents = form.getFieldValue('ocf_agents') || [];
        const newAgents = arrayMove(ocfAgents, oldIndex, newIndex);
        form.setFieldValue('ocf_agents', newAgents);
      }
    }
  };

  // Add new agent
  const handleAddAgent = () => {
    if (addAgentType === 'mount') {
      const newItem: OcfAgentItemData = {
        type: 'mount',
        original: '',
        value: resources[0]?.name ? `${resources[0].name}.mount` : '',
      };
      const newItems = [...items, newItem];
      setItems(newItems);

      const ocfAgents = form.getFieldValue('ocf_agents') || [];
      form.setFieldValue('ocf_agents', [...ocfAgents, newItem]);
      closeAddModal();
    } else if (addAgentType === 'service') {
      const newItem: OcfAgentItemData = {
        type: 'service',
        original: '',
        value: '',
      };
      const newItems = [...items, newItem];
      setItems(newItems);

      const ocfAgents = form.getFieldValue('ocf_agents') || [];
      form.setFieldValue('ocf_agents', [...ocfAgents, newItem]);
      closeAddModal();
    } else if (addAgentType === 'ocf') {
      if (!selectedProvider || !selectedAgent) {
        message.error('Please select provider and agent');
        return;
      }

      const providerAgents = allAgents?.providers[selectedProvider] || [];
      const agentMetadata = providerAgents.find((a) => a.name === selectedAgent);

      if (!agentMetadata) {
        message.error('Agent metadata not found');
        return;
      }

      // Generate default params
      const defaultParams: Record<string, string> = {};
      agentMetadata.parameters.forEach((param) => {
        if (param.required || (param.default && param.default !== '')) {
          defaultParams[param.name] = param.default || '';
        }
      });

      const instanceName = `${selectedAgent}_new`;

      const newItem: OcfAgentItemData = {
        type: 'ocf',
        original: `ocf:${selectedProvider}:${selectedAgent} ${instanceName}`,
        provider: selectedProvider,
        agent_type: selectedAgent,
        instance_name: instanceName,
        params: defaultParams,
      };

      const newItems = [...items, newItem];
      setItems(newItems);

      const ocfAgents = form.getFieldValue('ocf_agents') || [];
      form.setFieldValue('ocf_agents', [...ocfAgents, newItem]);
      closeAddModal();
    }
  };

  const openAddModal = () => {
    setAddModalVisible(true);
    setAddAgentType('ocf');
    setSelectedProvider('');
    setSelectedAgent('');
  };

  const closeAddModal = () => {
    setAddModalVisible(false);
  };

  // Generate TOML preview
  const generateTomlPreview = (): string => {
    const ocfAgents = form.getFieldValue('ocf_agents') || [];

    const agentStrings = ocfAgents.map((agent: OcfAgentItemData, index: number) => {
      if (agent.type === 'ocf') {
        const params = Object.entries(agent.params || {})
          .filter(([_, value]) => value !== undefined && value !== '')
          .map(([key, value]) => {
            if (String(value).includes(' ') || String(value).includes(',') || String(value) === '') {
              return `${key}='${value}'`;
            }
            return `${key}=${value}`;
          })
          .join(' ');

        return `    "${agent.provider}:${agent.agent_type} ${agent.instance_name}${params ? ' ' + params : ''}"`;
      } else if (agent.type === 'mount') {
        return `    "${agent.value}"`;
      } else if (agent.type === 'service') {
        return `    "${agent.value}"`;
      }
      return '';
    });

    if (agentStrings.length === 0) {
      return 'start = []';
    }

    return `start = [\n${agentStrings.join(',\n')}\n  ]`;
  };

  // Generate IDs for DnD
  const itemIds = items.map((_, index) => `agent-${index}`);

  return (
    <div>
      <Divider>Start Array Configuration</Divider>

      <div style={{ display: 'flex', gap: '16px', marginTop: '16px' }}>
        {/* Left Panel - Editor */}
        <div style={{ flex: 1 }}>
          {items.length === 0 ? (
            <div
              style={{
                textAlign: 'center',
                padding: '40px',
                border: `2px dashed ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
                borderRadius: '8px',
              }}
            >
              <Text type="secondary">No items in start array</Text>
              <br />
              <Text type="secondary" style={{ fontSize: '12px' }}>
                Add mount units, services, or OCF agents
              </Text>
            </div>
          ) : (
            <DndContext
              sensors={sensors}
              collisionDetection={closestCenter}
              onDragEnd={handleDragEnd}
            >
              <SortableContext items={itemIds} strategy={verticalListSortingStrategy}>
                {items.map((item, index) => {
                  const id = `agent-${index}`;
                  return (
                    <SortableAgentItem
                      key={id}
                      id={id}
                      index={index}
                      item={item}
                      resources={resources}
                      services={services}
                      currentTheme={currentTheme}
                      allAgents={allAgents}
                      onDelete={deleteAgent}
                      onExpand={toggleExpand}
                      expandedKeys={expandedKeys}
                      onRemoveParam={handleRemoveParam}
                      onAddParam={handleAddParam}
                      onUpdateItem={() => {}}
                      form={form}
                    />
                  );
                })}
              </SortableContext>
            </DndContext>
          )}

          <Button
            type="dashed"
            onClick={openAddModal}
            block
            icon={<PlusOutlined />}
            style={{ marginTop: '16px' }}
          >
            Add Item to Start Array
          </Button>
        </div>

        {/* Right Panel - Live Preview */}
        <div
          style={{
            flex: 1,
            background: currentTheme === 'dark' ? '#0f172a' : '#f1f5f9',
            borderRadius: '8px',
            padding: '16px',
            border: `1px solid ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
          }}
        >
          <Text strong style={{ display: 'block', marginBottom: '12px' }}>
            Live Preview (TOML)
          </Text>
          <pre
            style={{
              fontFamily: 'monospace',
              fontSize: '13px',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
              margin: 0,
            }}
          >
            {generateTomlPreview()}
          </pre>
        </div>
      </div>

      {/* Add Agent Modal */}
      <Modal
        title="Add Item to Start Array"
        open={addModalVisible}
        onOk={handleAddAgent}
        onCancel={closeAddModal}
        width={600}
        okText="Add"
        cancelText="Cancel"
      >
        <Form layout="vertical" style={{ marginTop: '16px' }}>
          <Form.Item label="Item Type">
            <Select
              value={addAgentType}
              onChange={setAddAgentType}
              options={[
                { label: 'Mount Unit', value: 'mount' },
                { label: 'Systemd Service', value: 'service' },
                { label: 'OCF Agent', value: 'ocf' },
              ]}
            />
          </Form.Item>

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
    </div>
  );
}
