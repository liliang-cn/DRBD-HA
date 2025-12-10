import {
  Card,
  Form,
  Radio,
  Divider,
  Input,
  InputNumber,
  Row,
  Col,
  Select,
  Checkbox,
  Space,
} from 'antd';
import type { FormInstance } from 'antd';
import {
  AppstoreOutlined,
  CloudServerOutlined,
  HddOutlined,
  ApiOutlined,
} from '@ant-design/icons';
import type { HaType, ServiceFileInfo } from '@/types';

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
  storageStrategy,
  resources,
  services,
}: HaConfigStepProps) {
  return (
    <Card
      title={
        mode === 'service'
          ? 'Step 3: Configure Service HA'
          : 'Step 3: Configure Storage Sharing'
      }
      className="max-w-4xl mx-auto"
    >
      <Form form={form} layout="vertical">
        <Form.Item
          label={mode === 'service' ? 'Service Type' : 'Storage Protocol'}
        >
          <Radio.Group
            value={haType}
            onChange={(e) => onHaTypeChange(e.target.value)}
            buttonStyle="solid"
          >
            {mode === 'service' ? (
              <Radio.Button value="generic">
                <AppstoreOutlined /> Application Service
              </Radio.Button>
            ) : (
              <>
                <Radio.Button value="nfs">
                  <CloudServerOutlined /> NFS
                </Radio.Button>
                <Radio.Button value="iscsi">
                  <HddOutlined /> iSCSI
                </Radio.Button>
                <Radio.Button value="nvmeof">
                  <ApiOutlined /> NVMe-oF
                </Radio.Button>
              </>
            )}
          </Radio.Group>
        </Form.Item>

        <Divider />

        <Form.Item
          name="name"
          label="HA Profile Name"
          rules={[{ required: true }]}
        >
          <Input placeholder="my-service-ha" />
        </Form.Item>

        {storageStrategy === 'lvm' ? (
          <Form.Item
            name="resource_name"
            label="DRBD Resource"
            tooltip="Resource name from storage configuration"
          >
            <Input disabled placeholder="Auto-filled from Step 2" />
          </Form.Item>
        ) : (
          <Form.Item
            name="resource_name"
            label="DRBD Resource"
            rules={[{ required: true }]}
          >
            <Select
              placeholder="Select DRBD resource"
              options={resources.map((r) => ({
                value: r.name,
                label: r.name,
              }))}
            />
          </Form.Item>
        )}

        {/* Generic & NFS fields */}
        {(haType === 'generic' || haType === 'nfs') && (
          <Row gutter={16}>
            <Col span={12}>
              <Form.Item
                name="mount_point"
                label={
                  haType === 'nfs' ? 'Export Path (Mount Point)' : 'Mount Point'
                }
                rules={[{ required: true }]}
              >
                <Input
                  placeholder={
                    haType === 'nfs' ? '/exports/share1' : '/var/lib/myservice'
                  }
                />
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
        )}

        {/* Generic Service Selection */}
        {haType === 'generic' && (
          <Form.Item
            name="services"
            label="Services"
            rules={[{ required: true }]}
          >
            <Select
              mode="tags"
              placeholder="Select or type services to manage (ordered)"
              options={services.map((s) => ({
                value: s.name,
                label: s.name,
              }))}
            />
          </Form.Item>
        )}

        {/* NFS Specific */}
        {haType === 'nfs' && (
          <>
            <Form.Item
              name="nfs_allowed_networks"
              label="Allowed Networks"
              initialValue="*"
            >
              <Input placeholder="e.g., 192.168.1.0/24, 10.0.0.0/8" />
            </Form.Item>
            <Form.Item
              name="nfs_options"
              label="NFS Options"
              initialValue="rw,sync,no_root_squash"
            >
              <Input />
            </Form.Item>
          </>
        )}

        {/* iSCSI Specific */}
        {haType === 'iscsi' && (
          <>
            <Form.Item
              name="iscsi_iqn"
              label="Target IQN"
              rules={[{ required: true }]}
            >
              <Input placeholder="iqn.2025-01.com.haforge:lun1" />
            </Form.Item>
            <Form.Item
              name="iscsi_allowed_initiators"
              label="Allowed Initiators (Optional)"
            >
              <Input placeholder="e.g., iqn.1991-05.com.microsoft:initiator1, ..." />
            </Form.Item>
          </>
        )}

        {/* NVMe-oF Specific */}
        {haType === 'nvmeof' && (
          <>
            <Form.Item
              name="nvmeof_nqn"
              label="Target NQN"
              rules={[{ required: true }]}
            >
              <Input placeholder="nqn.2025-01.com.haforge:subsys1" />
            </Form.Item>
            <Row gutter={16}>
              <Col span={12}>
                <Form.Item
                  name="nvmeof_fabric_type"
                  label="Fabric Type"
                  initialValue="tcp"
                >
                  <Select options={[{ value: 'tcp' }, { value: 'rdma' }]} />
                </Form.Item>
              </Col>
              <Col span={12}>
                <Form.Item
                  name="nvmeof_trsvcid"
                  label="Port (Service ID)"
                  initialValue="4420"
                >
                  <Input />
                </Form.Item>
              </Col>
            </Row>
            <Form.Item
              name="nvmeof_allowed_nqns"
              label="Allowed Host NQNs (Optional)"
            >
              <Input placeholder="e.g., nqn.2014-08.org.nvmexpress:uuid:..." />
            </Form.Item>
          </>
        )}

        <Divider>VIP Configuration (Optional)</Divider>
        <Row gutter={16}>
          <Col span={8}>
            <Form.Item name="vip_address" label="VIP Address">
              <Input placeholder="192.168.1.100" />
            </Form.Item>
          </Col>
          <Col span={8}>
            <Form.Item name="vip_netmask" label="Netmask" initialValue={24}>
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

        {(haType === 'generic' || haType === 'nfs') && (
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
                Migrate existing data to DRBD volume
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
                    <Form.Item
                      name="source_path"
                      label="Source Directory"
                      rules={[{ required: true }]}
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
