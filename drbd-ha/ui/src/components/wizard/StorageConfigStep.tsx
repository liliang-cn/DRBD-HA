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
} from 'antd';
import type { FormInstance } from 'antd';
import type { Node, BlockDevice, StoragePool } from '@/types';

interface StorageConfigStepProps {
  form: FormInstance;
  storageStrategy: 'raw' | 'lvm';
  onStrategyChange: (strategy: 'raw' | 'lvm') => void;
  nodes: Node[];
  availableDisks: Record<string, BlockDevice[]>;
  storagePools: StoragePool[];
  refreshPools?: () => void;
}

export function StorageConfigStep({
  form,
  nodes,
  availableDisks,
}: StorageConfigStepProps) {
  return (
    <Card title="Step 2: Storage Configuration" className="max-w-4xl mx-auto">
      <Form form={form} layout="vertical">
        <Form.Item
          name="name"
          label="Resource Name"
          rules={[{ required: true }]}
        >
          <Input placeholder="ha-data" />
        </Form.Item>

        <Row gutter={16}>
          <Col span={12}>
            <Form.Item
              name="port"
              label="DRBD Port"
              rules={[{ required: true }]}
              initialValue={7788}
            >
              <InputNumber min={1024} max={65535} className="w-full" />
            </Form.Item>
          </Col>
          <Col span={12}>
            <Form.Item
              name="minor"
              label="Minor Number"
              rules={[{ required: true }]}
              initialValue={0}
            >
              <InputNumber min={0} className="w-full" />
            </Form.Item>
          </Col>
        </Row>

        <Form.Item name="fs_type" label="Filesystem Type" initialValue="xfs">
          <Select
            options={[{ value: 'xfs' }, { value: 'ext4' }, { value: 'btrfs' }]}
          />
        </Form.Item>
        <Divider>Node Disks</Divider>
        {nodes.map((node) => (
          <Form.Item
            key={node.id}
            name={['node_disks', node.id]}
            label={`${node.hostname} (${node.ip})`}
            rules={[{ required: true, message: 'Select a disk' }]}
          >
            <Select
              placeholder="Select disk"
              options={(availableDisks[node.id] || []).map((d) => ({
                value: d.path,
                label: `${d.path} (${d.size_human})`,
              }))}
            />
          </Form.Item>
        ))}

        <Divider />
        <Form.Item
          name="force"
          valuePropName="checked"
          initialValue={false}
          tooltip="Bypass safety checks (e.g. if device is already configured)"
        >
          <Checkbox className="text-red-500">
            Force creation (ignore safety checks)
          </Checkbox>
        </Form.Item>
      </Form>
    </Card>
  );
}