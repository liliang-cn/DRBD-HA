import { Form, Modal, Select, Tag, Typography } from 'antd';
import type { OcfAgentWithMetadata } from '@/api/ha-profiles';

const { Text } = Typography;

interface AddParameterModalProps {
  visible: boolean;
  onOk: () => void;
  onCancel: () => void;
  currentAgentIndex: number | null;
  selectedParam: string;
  onParamChange: (param: string) => void;
  parsedAgents: OcfAgentWithMetadata[];
}

export function AddParameterModal({
  visible,
  onOk,
  onCancel,
  currentAgentIndex,
  selectedParam,
  onParamChange,
  parsedAgents,
}: AddParameterModalProps) {
  return (
    <Modal
      title="Add Parameter"
      open={visible}
      onOk={onOk}
      onCancel={onCancel}
      width={600}
      okText="Add"
      cancelText="Cancel"
    >
      <Form layout="vertical" style={{ marginTop: '16px' }}>
        <Form.Item label="Parameter">
          <Select
            placeholder="Select parameter to add"
            value={selectedParam || undefined}
            onChange={onParamChange}
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
                  background: '#1e293b',
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
  );
}
