import {
  InfoCircleOutlined,
  SaveOutlined,
  SyncOutlined,
} from '@ant-design/icons';
import {
  Button,
  Card,
  Divider,
  Form,
  message,
  Space,
  Tabs,
  Tag,
  Typography,
} from 'antd';
import { useEffect, useState } from 'react';
import { haProfilesApi } from '@/api';
import type {
  OcfAgentWithMetadata,
  TomlSection,
  TomlWithAgentsResponse,
} from '@/api/ha-profiles';
import AgentParamsEditor from '@/components/agent-editor/AgentParamsEditor';
import { useThemeStore } from '@/stores/theme';

const { Title, Text } = Typography;

interface TomlFormEditorProps {
  profile: { name: string; id: string } | null;
  onSave?: () => void;
  onCancel?: () => void;
}

export function TomlFormEditor({ profile, onSave, onCancel }: TomlFormEditorProps) {
  const { theme: currentTheme } = useThemeStore();
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [parsedData, setParsedData] = useState<TomlWithAgentsResponse | null>(null);

  // Load TOML content when profile changes
  useEffect(() => {
    if (profile) {
      loadToml();
    }
  }, [profile]);

  const loadToml = async () => {
    if (!profile) return;

    setLoading(true);
    try {
      // Use new backend API that parses TOML and fetches agent metadata
      const result = await haProfilesApi.parseToml(profile.name);
      setParsedData(result);
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = async () => {
    if (!profile) return;

    message.info('Save functionality will be implemented with form data');
    onSave?.();
  };

  const handleSync = async () => {
    if (!profile) return;

    setSyncing(true);
    try {
      const result = await haProfilesApi.syncToml(profile.name);

      if (result.success) {
        message.success(
          `TOML configuration synced to ${result.syncedNodes.length} node(s): ${result.syncedNodes.join(', ')}`
        );
      } else {
        message.info(result.message);
      }
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      setSyncing(false);
    }
  };

  const renderOcfAgentCard = (agentWithMeta: OcfAgentWithMetadata, index: number) => {
    const { agent, metadata } = agentWithMeta;

    return (
      <Card
        key={`${agentWithMeta.position.section}-${agentWithMeta.position.key}-${index}`}
        size="small"
        title={
          <Space>
            <Tag color="blue">OCF Agent</Tag>
            <span className="font-semibold">
              {agent.provider}:{agent.agent_type}
            </span>
            <Tag color="purple">{agent.instance_name}</Tag>
          </Space>
        }
        style={{
          background: currentTheme === 'dark' ? '#1e293b' : '#f0f9ff',
          border: '1px solid #bae6fd',
        }}
      >
        {metadata ? (
          <AgentParamsEditor
            agent={metadata}
            values={agent.params}
            readOnly={false}
            onChange={(newValues) => {
              console.log('Agent params changed:', newValues);
              // TODO: Update parsedData with new values
            }}
          />
        ) : (
          <div>
            <Text type="secondary">Failed to load agent metadata</Text>
            <pre className="mt-2 text-xs bg-gray-100 dark:bg-gray-800 p-2 rounded">
              {JSON.stringify(agent.params, null, 2)}
            </pre>
          </div>
        )}
      </Card>
    );
  };

  const renderRegularItem = (
    sectionName: string,
    itemKey: string,
    value: string,
    index?: number
  ) => {
    const formKey = index !== undefined
      ? `${sectionName}_${index}_${itemKey}`
      : `${sectionName}.${itemKey}`;

    return (
      <div key={`${sectionName}-${itemKey}-${index}`} className="mb-3">
        <div className="flex items-center gap-2 mb-1">
          <span className="font-medium">{itemKey}</span>
          <Tag color="green">string</Tag>
        </div>
        <div
          className="p-2 rounded border border-gray-300 dark:border-gray-600 bg-gray-50 dark:bg-gray-800"
          style={{ fontFamily: 'monospace', fontSize: '12px' }}
        >
          {value}
        </div>
      </div>
    );
  };

  const renderSection = (section: TomlSection) => {
    // Group OCF agents by their parent section
    const sectionAgents = parsedData!.content.ocf_agents.filter(
      agent => agent.position.section === section.name
    );

    // Group items by start/stop arrays
    const startAgents = sectionAgents.filter(a => a.position.key === 'start');
    const stopAgents = sectionAgents.filter(a => a.position.key === 'stop');

    return (
      <div key={section.name} className="space-y-4">
        <Title level={4} className="!mb-4">
          {section.is_array ? `[[${section.name}]]` : `[${section.name}]`}
        </Title>

        <div className="space-y-3">
          {/* Render start array agents */}
          {startAgents.length > 0 && (
            <Card
              size="small"
              title={
                <Space>
                  <span className="font-semibold">start</span>
                  <Tag color="blue">Array ({startAgents.length} agents)</Tag>
                </Space>
              }
              style={{
                background: currentTheme === 'dark' ? '#1e293b' : '#f8fafc',
              }}
            >
              <div className="space-y-3">
                {startAgents.map((agentWithMeta, idx) =>
                  renderOcfAgentCard(agentWithMeta, idx)
                )}
              </div>
            </Card>
          )}

          {/* Render stop array agents */}
          {stopAgents.length > 0 && (
            <Card
              size="small"
              title={
                <Space>
                  <span className="font-semibold">stop</span>
                  <Tag color="orange">Array ({stopAgents.length} agents)</Tag>
                </Space>
              }
              style={{
                background: currentTheme === 'dark' ? '#1e293b' : '#f8fafc',
              }}
            >
              <div className="space-y-3">
                {stopAgents.map((agentWithMeta, idx) =>
                  renderOcfAgentCard(agentWithMeta, idx)
                )}
              </div>
            </Card>
          )}

          {/* Render regular TOML items (non-OCF) */}
          {section.items
            .filter(item => !item.is_ocf_agent && item.key !== 'start' && item.key !== 'stop')
            .map(item => renderRegularItem(section.name, item.key, item.value))}
        </div>
      </div>
    );
  };

  if (loading) {
    return (
      <div style={{ textAlign: 'center', padding: '40px' }}>
        <Text>Loading TOML configuration...</Text>
      </div>
    );
  }

  if (!parsedData) {
    return (
      <div style={{ textAlign: 'center', padding: '40px' }}>
        <Text type="secondary">No configuration loaded</Text>
      </div>
    );
  }

  return (
    <div className="toml-form-editor">
      <Tabs
        type="card"
        items={parsedData.content.sections.map(section => ({
          key: section.name,
          label: (
            <span>
              {section.name}
              {section.is_array && (
                <Tag className="ml-2" color="blue">
                  Array
                </Tag>
              )}
            </span>
          ),
          children: renderSection(section),
        }))}
      />

      <Divider />

      <Space style={{ width: '100%', justifyContent: 'flex-end' }}>
        {onCancel && <Button onClick={onCancel}>Cancel</Button>}
        <Button
          icon={<SyncOutlined spin={syncing} />}
          onClick={handleSync}
          disabled={syncing}
        >
          {syncing ? 'Syncing...' : 'Sync to Nodes'}
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
      </Space>
    </div>
  );
}
