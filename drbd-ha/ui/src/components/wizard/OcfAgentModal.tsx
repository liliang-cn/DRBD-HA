import { Modal, Steps, Button, Table, Input, Form, Select, Spin, message, Typography, Space, Tooltip } from 'antd';
import { useState, useEffect } from 'react';
import { InfoCircleOutlined } from '@ant-design/icons';
import { resourceAgentsApi, AgentSummary, ResourceAgent } from '@/api/resource-agents';
import { OcfAgentConfig } from '@/types';

const { Search } = Input;
const { Text } = Typography;

interface OcfAgentModalProps {
  visible: boolean;
  onCancel: () => void;
  onAdd: (config: OcfAgentConfig) => void;
}

export function OcfAgentModal({ visible, onCancel, onAdd }: OcfAgentModalProps) {
  const [step, setStep] = useState(0);
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [filteredAgents, setFilteredAgents] = useState<AgentSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedAgentSummary, setSelectedAgentSummary] = useState<AgentSummary | null>(null);
  const [metadata, setMetadata] = useState<ResourceAgent | null>(null);
  const [form] = Form.useForm();
  const [searchText, setSearchText] = useState('');

  useEffect(() => {
    if (visible && step === 0) {
      loadAgents();
    }
  }, [visible, step]);

  useEffect(() => {
    if (searchText) {
      const lower = searchText.toLowerCase();
      setFilteredAgents(agents.filter(a => 
        a.name.toLowerCase().includes(lower) || 
        a.provider.toLowerCase().includes(lower)
      ));
    } else {
      setFilteredAgents(agents);
    }
  }, [searchText, agents]);

  const loadAgents = async () => {
    setLoading(true);
    try {
      const list = await resourceAgentsApi.list();
      setAgents(list);
      setFilteredAgents(list);
    } catch (err) {
      message.error('Failed to load resource agents');
    } finally {
      setLoading(false);
    }
  };

  const handleAgentSelect = async (agent: AgentSummary) => {
    setSelectedAgentSummary(agent);
    setLoading(true);
    try {
      const meta = await resourceAgentsApi.getMetadata(agent.provider, agent.name);
      setMetadata(meta);
      setStep(1);
      // Set default values
      const defaultValues: Record<string, string> = {};
      meta.parameters.parameter.forEach(p => {
        if (p.content['@default']) {
          defaultValues[p['@name']] = p.content['@default'];
        }
      });
      // Also set instance name default
      defaultValues['instance_name'] = `${agent.name}_1`;
      form.setFieldsValue(defaultValues);
    } catch (err) {
      message.error('Failed to load agent metadata');
    } finally {
      setLoading(false);
    }
  };

  const handleFinish = async () => {
    try {
      const values = await form.validateFields();
      const instanceName = values.instance_name;
      const params: Record<string, string> = {};
      
      // Filter out instance_name from params and ignore empty optional params
      Object.entries(values).forEach(([key, value]) => {
        if (key !== 'instance_name' && value !== undefined && value !== '') {
          params[key] = String(value);
        }
      });

      if (selectedAgentSummary) {
        onAdd({
          name: `ocf:${selectedAgentSummary.provider}:${selectedAgentSummary.name}`,
          instance_name: instanceName,
          params
        });
        reset();
      }
    } catch (err) {
      // Validation failed
    }
  };

  const reset = () => {
    setStep(0);
    setSelectedAgentSummary(null);
    setMetadata(null);
    form.resetFields();
    setSearchText('');
  };

  const handleCancel = () => {
    reset();
    onCancel();
  };

  const renderAgentList = () => (
    <div className="flex flex-col h-[400px]">
      <div className="mb-4">
        <Search 
          placeholder="Search agents..." 
          allowClear 
          onChange={e => setSearchText(e.target.value)} 
        />
      </div>
      <div className="flex-1 overflow-auto">
        <Table 
          dataSource={filteredAgents} 
          rowKey={record => `${record.provider}:${record.name}`}
          size="small"
          pagination={false}
          loading={loading}
          columns={[
            { title: 'Provider', dataIndex: 'provider', width: 100 },
            { title: 'Name', dataIndex: 'name' },
            { 
              title: 'Action', 
              key: 'action',
              width: 80,
              render: (_, record) => (
                <Button type="link" size="small" onClick={() => handleAgentSelect(record)}>
                  Select
                </Button>
              )
            }
          ]}
        />
      </div>
    </div>
  );

  const renderConfigForm = () => {
    if (!metadata) return null;

    return (
      <Form form={form} layout="vertical" className="max-h-[400px] overflow-auto pr-2">
        <div className="mb-4 p-3 bg-gray-50 rounded">
          <Text strong>{metadata.name}</Text>
          <div className="text-gray-500 text-sm">{metadata.shortdesc.text}</div>
          <div className="text-gray-400 text-xs mt-1">{metadata.longdesc.text}</div>
        </div>

        <Form.Item
          name="instance_name"
          label="Instance Name"
          rules={[{ required: true, message: 'Instance name is required' }]}
          help="Unique identifier for this agent instance"
        >
          <Input />
        </Form.Item>

        <Divider orientation="left" plain>Parameters</Divider>

        {metadata.parameters.parameter.map(param => (
          <Form.Item
            key={param['@name']}
            name={param['@name']}
            label={
              <Space>
                {param['@name']}
                {param['@required'] === '1' && <span className="text-red-500">*</span>}
                <Tooltip title={param.longdesc.text}>
                  <InfoCircleOutlined className="text-gray-400" />
                </Tooltip>
              </Space>
            }
            help={param.shortdesc.text}
            rules={[{ required: param['@required'] === '1', message: 'Required' }]}
          >
            {param.content['@type'] === 'boolean' ? (
              <Select>
                <Select.Option value="true">true</Select.Option>
                <Select.Option value="false">false</Select.Option>
              </Select>
            ) : param.content['@type'] === 'integer' ? (
              <Input type="number" />
            ) : (
              <Input />
            )}
          </Form.Item>
        ))}
      </Form>
    );
  };

  return (
    <Modal
      title="Add OCF Resource Agent"
      open={visible}
      onCancel={handleCancel}
      width={700}
      footer={
        step === 1 ? (
          <div className="flex justify-between">
            <Button onClick={() => setStep(0)}>Back</Button>
            <Button type="primary" onClick={handleFinish}>Add Agent</Button>
          </div>
        ) : null
      }
    >
      <Steps current={step} items={[{ title: 'Select Agent' }, { title: 'Configure' }]} className="mb-4" />
      
      {step === 0 ? renderAgentList() : renderConfigForm()}
    </Modal>
  );
}

import { Divider } from 'antd';
