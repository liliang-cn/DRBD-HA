import {
  Card,
  Form,
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

export function HaConfigStep({ form, resources, services }: HaConfigStepProps) {
  return (
    <Card title="Step 3: Configure Service HA" className="max-w-4xl mx-auto">
      <Form form={form} layout="vertical">
        {/* Implicitly Generic HA Type */}

        <Form.Item
          name="name"
          label="HA Profile Name"
          rules={[{ required: true }]}
        >
          <Input placeholder="my-service-ha" />
        </Form.Item>

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

        <Row gutter={16}>
          <Col span={12}>
            <Form.Item
              name="mount_point"
              label="Mount Point"
              rules={[{ required: true }]}
            >
              <Input placeholder="/var/lib/myservice" />
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

        {/* Generic Service Selection */}
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
      </Form>
    </Card>
  );
}