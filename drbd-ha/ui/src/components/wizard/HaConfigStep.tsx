import {
  DeleteOutlined,
  PlusOutlined,
  SettingOutlined,
} from '@ant-design/icons';
import type { FormInstance } from 'antd';
import {
  Button,
  Card,
  Checkbox,
  Col,
  Collapse,
  Divider,
  Form,
  Input,
  InputNumber,
  List,
  Radio,
  Row,
  Select,
  Space,
  Tag,
  Typography,
} from 'antd';
import { useEffect, useState } from 'react';
import { useNodesStore } from '@/stores/nodes';
import type {
  HaType,
  Node,
  OcfAgentConfig,
  ServiceFileInfo,
} from '@/types';
import { OcfAgentModal } from './OcfAgentModal';

const { Text } = Typography;

// Define Resource type locally
interface Resource {
  name: string;
  id?: string;
}

interface HaConfigStepProps {
  form: FormInstance;
  mode?: 'service' | 'storage';
  haType: HaType;
  onHaTypeChange: (type: HaType) => void;
  storageStrategy: 'raw' | 'lvm';
  resources: Resource[];
  services: ServiceFileInfo[];
}

export function HaConfigStep({
  form,
  mode = 'service',
  haType,
  onHaTypeChange,
  resources,
  services,
}: HaConfigStepProps) {
  const [showAgentModal, setShowAgentModal] = useState(false);
  const { nodes } = useNodesStore();
  const [selectedResource, setSelectedResource] = useState<string | null>(null);
  const mountStrategy = Form.useWatch('mount_strategy', form);

  // Available nodes for preferred nodes selection
  const [availableNodes, setAvailableNodes] = useState<
    Array<{ key: string; title: string }>
  >([]);

  // Update available nodes when nodes data changes
  useEffect(() => {
    if (nodes && nodes.length > 0) {
      const nodeOptions = nodes.map((node) => ({
        key: node.hostname,
        title: `${node.hostname} (${node.ip})`,
      }));
      setAvailableNodes(nodeOptions);
    }
  }, [nodes]);

  // Set default values when type changes
  useEffect(() => {
    // No default values needed for generic HA
  }, [haType, form]);

  return (
    <Card
      title="Step 3: Configure Service HA"
      className="max-w-4xl mx-auto"
    >
      <Form form={form} layout="vertical">

        <Form.Item
          name="name"
          label="Profile Name"
          rules={[{ required: true, message: 'Please enter a profile name' }]}
        >
          <Input placeholder="my-service-ha" />
        </Form.Item>

        <Form.Item
          name="resource_name"
          label="DRBD Resource"
          rules={[{ required: true, message: 'Please select a DRBD resource' }]}
          extra={
            selectedResource && (
              <Space direction="vertical" size="small">
                <Text type="secondary" style={{ fontSize: '12px' }}>
                  📄 Device: <code>/dev/drbd/by-{selectedResource}/0</code>
                </Text>
                <Text type="secondary" style={{ fontSize: '12px' }}>
                  📁 Mount Point:{' '}
                  <code>
                    /dev/drbd
                    {selectedResource ? selectedResource.slice(-1) : '0'}
                  </code>{' '}
                  (alternative path)
                </Text>
              </Space>
            )
          }
        >
          <Select
            placeholder="Select DRBD resource"
            options={resources.map((r) => ({
              value: r.name,
              label: r.name,
            }))}
            onChange={(value) => {
              setSelectedResource(value);
              form.setFieldValue('resource_name', value);
            }}
          />
        </Form.Item>

        {/* Mount Point for Generic HA */}
        <Row gutter={16}>
          <Col span={12}>
            <Form.Item
              name="mount_point"
              label="Mount Point"
              rules={[
                { required: true, message: 'Mount point is required' },
              ]}
              help={
                mountStrategy === 'ocf'
                  ? 'This path will be used as the "directory" parameter for the automatically generated OCF Filesystem agent.'
                  : undefined
              }
            >
              <Input placeholder="/srv/myapp/data" />
            </Form.Item>
          </Col>
          <Col span={12}>
            <Form.Item name="fs_type" label="Filesystem" initialValue="xfs">
              <Select
                options={[
                  { value: 'xfs' },
                  { value: 'ext4' },
                  { value: 'btrfs' },
                ]}
              />
            </Form.Item>
          </Col>
        </Row>

        {/* Mount Strategy */}
        <Form.Item
          name="mount_strategy"
          label="Mount Strategy"
          initialValue="systemd"
          help={
            <Space direction="vertical" size="small">
              <Text type="secondary">
                <strong>Systemd (Recommended):</strong> Uses systemd mount
                units. Best for databases and simple setups.
              </Text>
              <Text type="secondary">
                <strong>OCF Filesystem Agent:</strong> Automatically
                configures an OCF Filesystem agent using the Mount Point
                above. Provides advanced monitoring and recovery.
              </Text>
            </Space>
          }
        >
          <Radio.Group>
            <Radio.Button value="systemd">
              Systemd Mount Unit
            </Radio.Button>
            <Radio.Button value="ocf">OCF Filesystem Agent</Radio.Button>
          </Radio.Group>
        </Form.Item>

  
        {/* --- Service Specific Fields (Generic) --- */}
        {haType === 'generic' && (
          <Form.Item
            name="services"
            label="Managed Services"
            rules={[
              { required: true, message: 'Please select at least one service' },
            ]}
            help="Select systemd services to be managed by this HA profile. They will be started/stopped with the resource."
          >
            <Select
              mode="tags"
              placeholder="Select or type services (e.g. mysql, nginx)"
              options={services.map((s) => ({
                value: s.name,
                label: s.name,
              }))}
            />
          </Form.Item>
        )}
        {/* No Storage Protocol Fields - Only Generic HA */}
        {/* --- Generic HA Specific Fields --- */}

        <Divider>Network Configuration (VIP)</Divider>
        <div className="bg-gray-50 p-4 rounded-md border border-gray-200">
          <Row gutter={16}>
            <Col span={10}>
              <Form.Item
                name="vip_address"
                label="Virtual IP Address"
                help="The floating IP clients will use to connect"
              >
                <Input placeholder="192.168.1.100" />
              </Form.Item>
            </Col>
            <Col span={6}>
              <Form.Item
                name="vip_netmask"
                label="Netmask (CIDR)"
                initialValue={24}
              >
                <InputNumber min={1} max={32} className="w-full" />
              </Form.Item>
            </Col>
            <Col span={8}>
              <Form.Item
                name="vip_interface"
                label="Interface"
                initialValue="eth0"
              >
                <Input />
              </Form.Item>
            </Col>
          </Row>
        </div>

        {/* --- OCF Agents --- */}
        <Divider>Additional Resource Agents</Divider>
        <Form.List name="ocf_agents">
          {(fields, { add, remove }) => (
            <>
              {fields.length > 0 && (
                <List
                  bordered
                  className="mb-4 bg-white"
                  dataSource={fields}
                  renderItem={(field, index) => {
                    const agent = form.getFieldValue([
                      'ocf_agents',
                      field.name,
                    ]) as OcfAgentConfig;
                    return (
                      <List.Item
                        actions={[
                          <Button
                            key="delete"
                            type="text"
                            danger
                            icon={<DeleteOutlined />}
                            onClick={() => remove(field.name)}
                          />,
                        ]}
                      >
                        <List.Item.Meta
                          title={`${agent.name} (${agent.instance_name})`}
                          description={
                            <Space size="small" wrap>
                              {Object.entries(agent.params).map(([k, v]) => (
                                <Text
                                  key={k}
                                  type="secondary"
                                  style={{ fontSize: '12px' }}
                                >
                                  {k}={v}
                                </Text>
                              ))}
                            </Space>
                          }
                        />
                      </List.Item>
                    );
                  }}
                />
              )}
              <Button
                type="dashed"
                onClick={() => setShowAgentModal(true)}
                block
                icon={<PlusOutlined />}
              >
                Add OCF Agent
              </Button>
              <OcfAgentModal
                visible={showAgentModal}
                onCancel={() => setShowAgentModal(false)}
                onAdd={(agent) => {
                  add(agent);
                  setShowAgentModal(false);
                }}
              />
            </>
          )}
        </Form.List>

        {/* --- Advanced Options --- */}
        <Collapse
          ghost
          items={[
            {
              key: 'advanced',
              label: (
                <Space>
                  <SettingOutlined />
                  <span>Advanced Options</span>
                </Space>
              ),
              children: (
                <div className="space-y-6">
                  {/* Preferred Nodes Configuration */}
                  <div className="bg-gray-50 p-4 rounded-md border border-gray-200">
                    <h4 className="font-semibold mb-3 text-gray-800">
                      Node Preferences
                    </h4>
                    <Text type="secondary" className="block mb-4 text-xs">
                      Select preferred nodes for running this service. The first
                      selected node has the highest priority.
                    </Text>

                    <Form.Item name="preferred_nodes" label="Preferred Nodes">
                      <Select
                        mode="multiple"
                        placeholder="Select preferred nodes (optional)"
                        allowClear
                        options={availableNodes}
                        optionFilterProp="title"
                      />
                    </Form.Item>

                    {form.getFieldValue('preferred_nodes')?.length > 0 && (
                      <>
                        <Form.Item
                          name="preferred_nodes_policy"
                          label="Preferred Nodes Policy"
                          initialValue="always"
                          help={
                            <Space direction="vertical" size="small">
                              <Text type="secondary">
                                <strong>Always:</strong> Always migrate to
                                higher priority nodes when they become available
                              </Text>
                              <Text type="secondary">
                                <strong>Start-only:</strong> Only consider
                                priority during initial service startup
                              </Text>
                            </Space>
                          }
                        >
                          <Radio.Group>
                            <Radio value="always">
                              Always prefer higher priority
                            </Radio>
                            <Radio value="start-only">
                              Start-only preference
                            </Radio>
                          </Radio.Group>
                        </Form.Item>

                        <Form.Item
                          name="sleep_before_promote_factor"
                          label="Promotion Delay Factor"
                          initialValue={1}
                          min={1}
                          max={10}
                          help="Multiplier for promotion delay based on node priority. Higher values increase the delay between priority levels."
                        >
                          <InputNumber
                            min={1}
                            max={10}
                            className="w-full"
                            addonAfter="×"
                          />
                        </Form.Item>
                      </>
                    )}
                  </div>

                  {/* Quorum and Failure Handling */}
                  <div className="bg-yellow-50 p-4 rounded-md border border-yellow-100">
                    <h4 className="font-semibold mb-3 text-yellow-800">
                      Quorum & Failure Handling
                    </h4>

                    <Form.Item
                      name="on_quorum_loss"
                      label="On Quorum Loss"
                      initialValue="shutdown"
                      help="Action to take when DRBD quorum is lost"
                    >
                      <Select
                        options={[
                          { value: 'shutdown', label: 'Shutdown (Default)' },
                          { value: 'freeze', label: 'Freeze services' },
                          {
                            value: 'ignore',
                            label: 'Ignore (Not recommended)',
                          },
                        ]}
                      />
                    </Form.Item>

                    <Form.Item
                      name="on_demote_failure"
                      label="On DRBD Demote Failure"
                      initialValue="reboot"
                      help="Action to take when DRBD demotion fails"
                    >
                      <Select
                        options={[
                          {
                            value: 'reboot-immediate',
                            label: 'Reboot immediately',
                          },
                          { value: 'reboot', label: 'Reboot (graceful)' },
                          { value: 'poweroff', label: 'Power off' },
                          {
                            value: 'ignore',
                            label: 'Ignore (Not recommended)',
                          },
                        ]}
                      />
                    </Form.Item>
                  </div>

                  {/* Dependency Configuration */}
                  <div className="bg-blue-50 p-4 rounded-md border border-blue-100">
                    <h4 className="font-semibold mb-3 text-blue-800">
                      Service Dependencies
                    </h4>

                    <Row gutter={16}>
                      <Col span={12}>
                        <Form.Item
                          name="dependencies_as"
                          label="Service Dependencies"
                          initialValue="Requires"
                          help="Type of dependency between services"
                        >
                          <Select
                            options={[
                              { value: 'Requires', label: 'Requires (Strict)' },
                              { value: 'Wants', label: 'Wants (Flexible)' },
                              { value: 'Requisite', label: 'Requisite' },
                            ]}
                          />
                        </Form.Item>
                      </Col>
                      <Col span={12}>
                        <Form.Item
                          name="target_as"
                          label="Target Dependencies"
                          initialValue="Requires"
                          help="Type of dependency for the target unit"
                        >
                          <Select
                            options={[
                              { value: 'Requires', label: 'Requires (Strict)' },
                              { value: 'Wants', label: 'Wants (Flexible)' },
                              { value: 'Requisite', label: 'Requisite' },
                            ]}
                          />
                        </Form.Item>
                      </Col>
                    </Row>
                  </div>
                </div>
              ),
            },
          ]}
        />

        {/* Data Migration for Generic HA */}
        <Divider>Data Migration</Divider>
        <Form.Item
          name="migrate_data"
          valuePropName="checked"
          initialValue={false}
        >
          <Checkbox
            onChange={(e) => {
              const checked = e.target.checked;
              if (checked && !form.getFieldValue('source_path')) {
                form.setFieldValue(
                  'source_path',
                  form.getFieldValue('mount_point'),
                );
              }
            }}
          >
            Migrate existing data to shared storage
          </Checkbox>
        </Form.Item>

        <Form.Item
          noStyle
          shouldUpdate={(prev, current) =>
            prev.migrate_data !== current.migrate_data
          }
        >
          {({ getFieldValue }) =>
            getFieldValue('migrate_data') ? (
              <div className="bg-gray-50 p-4 rounded-md mb-4 border border-gray-200">
                <Text type="secondary" className="block mb-4 text-xs">
                  This will copy data from the source directory to the new
                  DRBD volume. Services might need to be stopped during this
                  process.
                </Text>
                <Form.Item
                  name="source_path"
                  label="Source Directory"
                  rules={[
                    {
                      required: true,
                      message: 'Source path is required for migration',
                    },
                  ]}
                >
                  <Input />
                </Form.Item>
                <Space size="large">
                  <Form.Item
                    name="format_device"
                    valuePropName="checked"
                    initialValue={true}
                    noStyle
                  >
                    <Checkbox>Format device before migration</Checkbox>
                  </Form.Item>
                  <Form.Item
                    name="preserve_permissions"
                    valuePropName="checked"
                    initialValue={true}
                    noStyle
                  >
                    <Checkbox>Preserve permissions</Checkbox>
                  </Form.Item>
                </Space>
              </div>
            ) : null
          }
        </Form.Item>
      </Form>
    </Card>
  );
}
