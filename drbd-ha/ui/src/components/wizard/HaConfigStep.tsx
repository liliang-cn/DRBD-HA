import {
  EyeOutlined,
  EyeInvisibleOutlined,
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
  Radio,
  Row,
  Select,
  Space,
  Typography,
} from 'antd';
import { useMemo, useState } from 'react';
import type { HaType, OcfAgentConfig, ServiceFileInfo } from '@/types';
import { OcfAgentEditor } from '@/components/ha/OcfAgentEditor';

const { Text } = Typography;

type ConfigMode = 'simple' | 'advanced';

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
  const mountStrategy = Form.useWatch('mount_strategy', form);
  const [configMode, setConfigMode] = useState<ConfigMode>('simple');
  const [showTomlPreview, setShowTomlPreview] = useState(false);
  const [ocfAgents, setOcfAgents] = useState<any[]>([]);

  // Watch form values for TOML preview - watch individual fields for better reactivity
  const name = Form.useWatch('name', form);
  const resourceName = Form.useWatch('resource_name', form);
  const mountPoint = Form.useWatch('mount_point', form);
  const fsType = Form.useWatch('fs_type', form);
  const selectedServices = Form.useWatch('services', form);
  const vipAddress = Form.useWatch('vip_address', form);
  const vipNetmask = Form.useWatch('vip_netmask', form);
  const preferredNodes = Form.useWatch('preferred_nodes', form);
  const preferredNodesPolicy = Form.useWatch('preferred_nodes_policy', form);
  const onQuorumLoss = Form.useWatch('on_quorum_loss', form);
  const onDemoteFailure = Form.useWatch('on_demote_failure', form);
  const stopOnDemote = Form.useWatch('stop_on_demote', form);
  const dependenciesAs = Form.useWatch('dependencies_as', form);
  const targetAs = Form.useWatch('target_as', form);
  const sleepBeforePromote = Form.useWatch('sleep_before_promote_factor', form);

  // Generate drbd-reactor TOML preview
  const tomlPreview = useMemo(() => {
    const resName = resourceName || 'r0';
    const mntPoint = mountPoint || '/mnt/data';
    const fst = fsType || 'xfs';
    const svc = selectedServices || [];
    const vipAddr = vipAddress;
    const vipNm = vipNetmask || 24;
    const agents = ocfAgents || [];
    const prefNodes = preferredNodes;
    const prefPolicy = preferredNodesPolicy;
    const quorumLoss = onQuorumLoss;
    const demoteFailure = onDemoteFailure || 'continue';
    const stopOnExit = stopOnDemote !== false;
    const depsAs = dependenciesAs;
    const tgtAs = targetAs;
    const sleepFactor = sleepBeforePromote;

    // Parse preferred_nodes string or array
    let preferredNodesList: string[] = [];
    if (typeof prefNodes === 'string') {
      preferredNodesList = prefNodes.split(/[\s,]+/).filter(n => n);
    } else if (Array.isArray(prefNodes)) {
      preferredNodesList = prefNodes;
    }

    // Convert mount point to systemd mount unit name
    // e.g., /var/lib/mysql -> var-lib-mysql.mount
    // systemd escape: - becomes --, then / becomes -
    const mountUnitName = mntPoint
      .slice(1)  // Remove leading /
      .replaceAll('-', '--')  // Escape existing dashes first
      .replaceAll('/', '-') + '.mount';  // Then replace slashes

    // Build start array
    const startEntries: string[] = [];

    if (configMode === 'simple') {
      // Simple mode: use systemd mount unit
      startEntries.push(`    "${mountUnitName}",`);
      startEntries.push('');  // blank line

      // VIP
      if (vipAddr) {
        startEntries.push(`    "ocf:heartbeat:IPaddr2 ${resName}_vip ip=${vipAddr} cidr_netmask=${vipNm}",`);
        startEntries.push('');  // blank line
      }

      // Services
      svc.forEach((service: string) => {
        startEntries.push(`    "${service}",`);
      });
    } else {
      // Advanced mode: use OCF agents
      agents.forEach((agent: any) => {
        // Handle different data formats from OCF Agent Editor
        if (!agent) return;

        if (agent.type === 'ocf') {
          // Format from OCF Agent Editor sync
          const agentName = `ocf:${agent.provider}:${agent.agent_type}`;
          const instanceName = agent.instance_name;
          const params = Object.entries(agent.params || {})
            .map(([k, v]) => `${k}=${v}`)
            .join(' ');
          startEntries.push(`    "${agentName} ${instanceName}${params ? ' ' + params : ''}",`);
        } else if (agent.type === 'mount' || agent.type === 'service' || agent.type === 'systemd') {
          // Systemd unit or service
          startEntries.push(`    "${agent.value}",`);
        } else {
          // Fallback: try to use as-is
          const agentName = agent.name || agent.agent_type || '';
          const instanceName = agent.instance_name || '';
          const params = Object.entries(agent.params || {})
            .map(([k, v]) => `${k}=${v}`)
            .join(' ');
          if (agentName) {
            startEntries.push(`    "${agentName} ${instanceName}${params ? ' ' + params : ''}",`);
          } else if (agent.value) {
            startEntries.push(`    "${agent.value}",`);
          }
        }
      });
    }

    const startArrayContent = startEntries.join('\n');
    const startArray = `start = [

${startArrayContent}

]`;

    // Build advanced options
    const advancedOptions: string[] = [];
    if (depsAs) advancedOptions.push(`dependencies-as = "${depsAs}"`);
    if (tgtAs) advancedOptions.push(`target-as = "${tgtAs}"`);
    if (quorumLoss) advancedOptions.push(`on-quorum-loss = "${quorumLoss}"`);
    if (preferredNodesList.length > 0) {
      const nodes = preferredNodesList.map(n => `"${n}"`).join(', ');
      advancedOptions.push(`preferred-nodes = [${nodes}]`);
    }
    if (prefPolicy) advancedOptions.push(`preferred-nodes-policy = "${prefPolicy}"`);
    if (sleepFactor) advancedOptions.push(`sleep-before-promote-factor = ${sleepFactor}`);

    const advancedSection = advancedOptions.length > 0
      ? '\n' + advancedOptions.join('\n')
      : '';

    const mountStrategyComment = configMode === 'simple' ? '\n\n# Mount strategy: systemd' : '';

    return `# drbd-reactor promoter configuration
# Generated by drbd-ha

[[promoter]]
[promoter.resources.${resName}]
${startArray}
runner = "systemd"
stop-services-on-exit = ${stopOnExit}
on-drbd-demote-failure = "${demoteFailure}"${advancedSection}${mountStrategyComment}
`;
  }, [name, resourceName, mountPoint, fsType, selectedServices, vipAddress, vipNetmask, ocfAgents, preferredNodes, preferredNodesPolicy, onQuorumLoss, onDemoteFailure, stopOnDemote, dependenciesAs, targetAs, sleepBeforePromote, configMode]);

  return (
    <Card
      title={
        <Space>
          <span>Step 3: Configure Service HA</span>
          <Button
            type="text"
            size="small"
            icon={showTomlPreview ? <EyeInvisibleOutlined /> : <EyeOutlined />}
            onClick={() => setShowTomlPreview(!showTomlPreview)}
            style={{ fontSize: 14 }}
          />
        </Space>
      }
      className="w-full"
    >
      <Row gutter={24} align="stretch">
        {/* Left: Form */}
        <Col span={showTomlPreview ? 12 : 24}>
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
        >
          <Select
            placeholder="Select DRBD resource"
            options={resources.map((r) => ({
              value: r.name,
              label: r.name,
            }))}
          />
        </Form.Item>

        {/* Configuration Mode Selector */}
        <Form.Item label="Configuration Mode">
          <Radio.Group value={configMode} onChange={(e) => setConfigMode(e.target.value)}>
            <Radio value="simple">Simple</Radio>
            <Radio value="advanced">Advanced (via OCF Agents)</Radio>
          </Radio.Group>
        </Form.Item>

        {/* Mount Strategy - Always systemd */}
        <Form.Item name="mount_strategy" initialValue="systemd" hidden>
          <Input />
        </Form.Item>

        {/* Simple Mode: Services, Mount Point, Filesystem, VIP */}
        {configMode === 'simple' && (
          <>
            <Form.Item
              name="services"
              label="Systemd Services"
              rules={[{ required: true, message: 'Please select at least one service' }]}
            >
              <Select
                mode="tags"
                placeholder="Select or type service names"
                options={services.map(s => ({ value: s.name, label: s.name }))}
              />
            </Form.Item>

            <Row gutter={16}>
              <Col span={12}>
                <Form.Item
                  name="mount_point"
                  label="Mount Point"
                  rules={[{ required: true, message: 'Mount point is required' }]}
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

            <Row gutter={16}>
              <Col span={16}>
                <Form.Item
                  name="vip_address"
                  label="VIP Address"
                  help="Virtual IP address for failover"
                >
                  <Input placeholder="192.168.1.100" />
                </Form.Item>
              </Col>
              <Col span={8}>
                <Form.Item
                  name="vip_netmask"
                  label="VIP Netmask"
                  initialValue={24}
                  help="CIDR notation"
                >
                  <InputNumber min={1} max={32} className="w-full" />
                </Form.Item>
              </Col>
            </Row>
          </>
        )}

        {/* Advanced Mode: OCF Agents */}
        {configMode === 'advanced' && (
          <>
            <Divider>OCF Agents</Divider>
            <div style={{ marginTop: '16px', marginBottom: '32px' }}>
              <OcfAgentEditor
                mode="create"
                externalForm={form}
                resources={resources}
                services={services.map(s => s.name)}
                onAgentsChange={setOcfAgents}
              />
            </div>
          </>
        )}

        <Divider>Advanced Options</Divider>

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
                      Enter preferred nodes for running this service (comma or space separated). The first
                      node has the highest priority.
                    </Text>

                    <Form.Item
                      name="preferred_nodes"
                      label="Preferred Nodes"
                      help="Example: orange1, orange2"
                    >
                      <Input
                        placeholder="Enter hostnames separated by commas or spaces"
                      />
                    </Form.Item>

                    {form.getFieldValue('preferred_nodes') && (
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
                  This will copy data from the source directory to the new DRBD
                  volume. Services might need to be stopped during this process.
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
        </Col>

        {/* Right: TOML Preview */}
        {showTomlPreview && (
          <Col span={12}>
            <div
              style={{
                border: '1px solid var(--ant-color-border)',
                borderRadius: 8,
                padding: 16,
                overflow: 'auto',
                height: '100%',
                background: 'var(--ant-colorBgContainer)',
              }}
            >
              <Text strong style={{ display: 'block', marginBottom: 12 }}>
                drbd-reactor TOML Preview
              </Text>
              <pre
                style={{
                  margin: 0,
                  fontFamily: 'Consolas, Monaco, "Andale Mono", monospace',
                  fontSize: 12,
                  lineHeight: 1.4,
                  color: 'var(--ant-colorText)',
                  whiteSpace: 'pre',
                }}
              >
                {tomlPreview || '# Fill in the form to see TOML preview'}
              </pre>
            </div>
          </Col>
        )}
      </Row>
    </Card>
  );
}
