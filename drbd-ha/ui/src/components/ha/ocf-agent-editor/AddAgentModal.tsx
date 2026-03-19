import { Form, Input, Modal, Select } from 'antd';
import type { ResourceAgentsByProvider } from '@/api/ha-profiles';

interface AddAgentModalProps {
  visible: boolean;
  onOk: () => void;
  onCancel: () => void;
  agentType: 'ocf' | 'systemd';
  onAgentTypeChange: (type: 'ocf' | 'systemd') => void;
  systemdUnit: string;
  onSystemdUnitChange: (value: string) => void;
  selectedProvider: string;
  onProviderChange: (provider: string) => void;
  selectedAgent: string;
  onAgentChange: (agent: string) => void;
  allAgents: ResourceAgentsByProvider | null;
  currentTheme: string;
}

export function AddAgentModal({
  visible,
  onOk,
  onCancel,
  agentType,
  onAgentTypeChange,
  systemdUnit,
  onSystemdUnitChange,
  selectedProvider,
  onProviderChange,
  selectedAgent,
  onAgentChange,
  allAgents,
  currentTheme,
}: AddAgentModalProps) {
  return (
    <Modal
      title="Add New Agent"
      open={visible}
      onOk={onOk}
      onCancel={onCancel}
      width={600}
      okText="Add"
      cancelText="Cancel"
    >
      <Form layout="vertical" style={{ marginTop: '16px' }}>
        <Form.Item label="Agent Type">
          <Select
            value={agentType}
            onChange={onAgentTypeChange}
            options={[
              { label: 'OCF Agent', value: 'ocf' },
              { label: 'Systemd Unit', value: 'systemd' },
            ]}
          />
        </Form.Item>

        {agentType === 'systemd' && (
          <Form.Item
            label="Systemd Unit"
            help="e.g., nginx.service, var-lib-mysql.mount"
          >
            <Input
              placeholder="Enter systemd unit name"
              value={systemdUnit}
              onChange={(e) => onSystemdUnitChange(e.target.value)}
              onPressEnter={onOk}
            />
          </Form.Item>
        )}

        {agentType === 'ocf' && (
          <>
            <Form.Item label="Provider">
              <Select
                placeholder="Select provider"
                value={selectedProvider || undefined}
                onChange={onProviderChange}
                options={
                  allAgents
                    ? Object.keys(allAgents.providers)
                        .sort()
                        .map((p) => ({
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
                onChange={onAgentChange}
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
                  (option?.label ?? '')
                    .toLowerCase()
                    .includes(input.toLowerCase())
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
                  {allAgents.providers[selectedProvider]?.find(
                    (a) => a.name === selectedAgent,
                  )?.longdesc || 'No description available'}
                </div>
              </Form.Item>
            )}
          </>
        )}
      </Form>
    </Modal>
  );
}
