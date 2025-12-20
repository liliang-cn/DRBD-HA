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
  Typography,
  Transfer,
  Tag,
} from 'antd';
import { useEffect, useState } from 'react';
import { DeleteOutlined, PlusOutlined, SettingOutlined } from '@ant-design/icons';
import type { HaType, OcfAgentConfig, ServiceFileInfo, Node } from '@/types';
import { OcfAgentModal } from './OcfAgentModal';
import { useNodesStore } from '@/stores/nodes';

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

  // Available nodes for preferred nodes selection
  const [availableNodes, setAvailableNodes] = useState<Array<{ key: string; title: string; }>>([]);

  // Update available nodes when nodes data changes
  useEffect(() => {
    if (nodes && nodes.length > 0) {
      const nodeOptions = nodes.map(node => ({
        key: node.hostname,
        title: `${node.hostname} (${node.ip})`,
      }));
      setAvailableNodes(nodeOptions);
    }
  }, [nodes]);

  // Set default values when type changes
  useEffect(() => {
    if (haType === 'iscsi' && !form.getFieldValue('iscsi_iqn')) {
      const year = new Date().getFullYear();
      const month = String(new Date().getMonth() + 1).padStart(2, '0');
      form.setFieldValue(
        'iscsi_iqn',
        `iqn.${year}-${month}.com.haforge:target1`,
      );
    } else if (haType === 'nvmeof' && !form.getFieldValue('nvmeof_nqn')) {
      const year = new Date().getFullYear();
      const month = String(new Date().getMonth() + 1).padStart(2, '0');
      form.setFieldValue(
        'nvmeof_nqn',
        `nqn.${year}-${month}.com.haforge:nvme1`,
      );
    }
  }, [haType, form]);

  const isStorageMode = mode === 'storage';
  const isBlockProtocol = haType === 'iscsi' || haType === 'nvmeof';

  return (
    <Card
      title={`Step 3: Configure ${isStorageMode ? 'Storage Sharing' : 'Service HA'}`}
      className="max-w-4xl mx-auto"
    >
      <Form form={form} layout="vertical">
        {/* Protocol Selection for Storage Mode */}
        {isStorageMode && (
          <Form.Item label="Sharing Protocol" className="mb-6">
            <Radio.Group
              value={haType}
              onChange={(e) => onHaTypeChange(e.target.value)}
              buttonStyle="solid"
            >
              <Radio.Button value="nfs">NFS</Radio.Button>
              <Radio.Button value="iscsi">iSCSI</Radio.Button>
              <Radio.Button value="nvmeof">NVMe-oF</Radio.Button>
            </Radio.Group>
          </Form.Item>
        )}

        <Form.Item
          name="name"
          label="Profile Name"
          rules={[{ required: true, message: 'Please enter a profile name' }]}
        >
          <Input
            placeholder={isStorageMode ? 'my-storage-share' : 'my-service-ha'}
          />
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
                  📁 Mount Point: <code>/dev/drbd{selectedResource ? selectedResource.slice(-1) : '0'}</code> (alternative path)
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

        {/* Mount Point - Only for File-based protocols (Generic, NFS) */}
        {!isBlockProtocol && (
          <>
            <Row gutter={16}>
              <Col span={12}>
                <Form.Item
                  name="mount_point"
                  label={
                    haType === 'nfs' ? 'Export Path (Mount Point)' : 'Mount Point'
                  }
                  rules={[{ required: true, message: 'Mount point is required' }]}
                  help={
                    haType === 'nfs'
                      ? 'The local path where the DRBD volume will be mounted and exported via NFS.'
                      : undefined
                  }
                >
                  <Input placeholder="/srv/nfs/share1" />
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

            {/* Mount Strategy - Advanced option for Storage mode */}
            {isStorageMode && (
              <Form.Item
                name="mount_strategy"
                label="Mount Strategy"
                initialValue="systemd"
                help={
                  <Space direction="vertical" size="small">
                    <Text type="secondary">
                      <strong>Systemd (Recommended):</strong> Uses systemd mount units, best for databases and most applications
                    </Text>
                    <Text type="secondary">
                      <strong>OCF Filesystem Agent:</strong> Advanced HA with monitoring and recovery, best for critical storage services
                    </Text>
                  </Space>
                }
              >
                <Radio.Group>
                  <Radio.Button value="systemd">Systemd Mount Unit</Radio.Button>
                  <Radio.Button value="ocf">OCF Filesystem Agent</Radio.Button>
                </Radio.Group>
              </Form.Item>
            )}
          </>
        )}

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

        {/* --- NFS Specific Fields --- */}
        {haType === 'nfs' && (
          <div className="bg-blue-50 p-4 rounded-md mb-4 border border-blue-100">
            <h4 className="font-semibold mb-3 text-blue-800">NFS Settings</h4>
            <Form.Item
              name="nfs_allowed_networks"
              label="Allowed Networks"
              initialValue="*"
              help="Comma separated list of IP addresses or CIDR networks (e.g. 192.168.1.0/24, 10.0.0.5)"
            >
              <Input placeholder="*" />
            </Form.Item>
            <Form.Item
              name="nfs_options"
              label="Export Options"
              initialValue="rw,sync,no_root_squash"
            >
              <Input />
            </Form.Item>
          </div>
        )}

        {/* --- iSCSI Specific Fields --- */}
        {haType === 'iscsi' && (
          <div className="bg-purple-50 p-4 rounded-md mb-4 border border-purple-100">
            <h4 className="font-semibold mb-3 text-purple-800">
              iSCSI Target Settings
            </h4>
            <Form.Item
              name="iscsi_iqn"
              label="Target IQN"
              rules={[{ required: true, message: 'IQN is required' }]}
              help="Unique iSCSI Qualified Name for this target"
            >
              <Input placeholder="iqn.2025-01.com.example:target1" />
            </Form.Item>
            <Form.Item
              name="iscsi_allowed_initiators"
              label="Allowed Initiators (ACLs)"
              help="Comma separated list of Initiator IQNs. Leave empty to allow all (not recommended for production)."
            >
              <Input.TextArea
                placeholder="iqn.1991-05.com.microsoft:host1, iqn.1994-05.com.redhat:host2"
                rows={2}
              />
            </Form.Item>
          </div>
        )}

        {/* --- NVMe-oF Specific Fields --- */}
        {haType === 'nvmeof' && (
          <div className="bg-orange-50 p-4 rounded-md mb-4 border border-orange-100">
            <h4 className="font-semibold mb-3 text-orange-800">
              NVMe-oF Target Settings
            </h4>
            <Row gutter={16}>
              <Col span={16}>
                <Form.Item
                  name="nvmeof_nqn"
                  label="Target NQN"
                  rules={[{ required: true, message: 'NQN is required' }]}
                >
                  <Input placeholder="nqn.2014-08.org.nvmexpress:uuid:..." />
                </Form.Item>
              </Col>
              <Col span={8}>
                <Form.Item
                  name="nvmeof_port"
                  label="Port (TRSVCID)"
                  initialValue="4420"
                >
                  <Input />
                </Form.Item>
              </Col>
            </Row>
            <Row gutter={16}>
              <Col span={12}>
                <Form.Item
                  name="nvmeof_fabric_type"
                  label="Fabric Type"
                  initialValue="tcp"
                >
                  <Select
                    options={[
                      { value: 'tcp', label: 'TCP' },
                      { value: 'rdma', label: 'RDMA' },
                    ]}
                  />
                </Form.Item>
              </Col>
            </Row>
            <Form.Item
              name="nvmeof_allowed_nqns"
              label="Allowed Host NQNs"
              help="Comma separated list of Host NQNs. Leave empty to allow all."
            >
              <Input.TextArea
                placeholder="nqn.2014-08.org.nvmexpress:uuid:client1..."
                rows={2}
              />
            </Form.Item>
          </div>
        )}

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
                    const agent = form.getFieldValue(['ocf_agents', field.name]) as OcfAgentConfig;
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
                                <Text key={k} type="secondary" style={{ fontSize: '12px' }}>
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
                    <h4 className="font-semibold mb-3 text-gray-800">Node Preferences</h4>
                    <Text type="secondary" className="block mb-4 text-xs">
                      Set node priority for service placement. Higher priority nodes will be preferred for running services.
                      Nodes are ordered by priority (first = highest priority).
                    </Text>

                    <Form.Item
                      name="preferred_nodes"
                      label="Preferred Nodes"
                    >
                      <Transfer
                        dataSource={availableNodes}
                        targetKeys={form.getFieldValue('preferred_nodes') || []}
                        onChange={(nextTargetKeys) => {
                          form.setFieldValue('preferred_nodes', nextTargetKeys);
                        }}
                        render={(item) => item.title}
                        titles={['Available Nodes', 'Preferred Nodes']}
                        oneWay
                        style={{ marginBottom: 16 }}
                        listStyle={{
                          width: 250,
                          height: 200,
                        }}
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
                                <strong>Always:</strong> Always migrate to higher priority nodes when they become available
                              </Text>
                              <Text type="secondary">
                                <strong>Start-only:</strong> Only consider priority during initial service startup
                              </Text>
                            </Space>
                          }
                        >
                          <Radio.Group>
                            <Radio value="always">Always prefer higher priority</Radio>
                            <Radio value="start-only">Start-only preference</Radio>
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
                    <h4 className="font-semibold mb-3 text-yellow-800">Quorum & Failure Handling</h4>

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
                          { value: 'ignore', label: 'Ignore (Not recommended)' },
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
                          { value: 'reboot-immediate', label: 'Reboot immediately' },
                          { value: 'reboot', label: 'Reboot (graceful)' },
                          { value: 'poweroff', label: 'Power off' },
                          { value: 'ignore', label: 'Ignore (Not recommended)' },
                        ]}
                      />
                    </Form.Item>
                  </div>

                  {/* Dependency Configuration */}
                  <div className="bg-blue-50 p-4 rounded-md border border-blue-100">
                    <h4 className="font-semibold mb-3 text-blue-800">Service Dependencies</h4>

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

        {/* Data Migration - Only for File-based protocols */}
        {!isBlockProtocol && (
          <>
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
          </>
        )}
      </Form>
    </Card>
  );
}
