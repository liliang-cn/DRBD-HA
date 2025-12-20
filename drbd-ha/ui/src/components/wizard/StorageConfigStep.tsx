import type { FormInstance } from 'antd';
import {
  Card,
  Checkbox,
  Col,
  Divider,
  Form,
  Input,
  InputNumber,
  Row,
  Select,
} from 'antd';
import type { BlockDevice, Node, StoragePool } from '@/types';
import { Radio } from 'antd';

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

        <Form.Item
          name="port"
          label="DRBD Port"
          rules={[{ required: true }]}
          initialValue={7788}
        >
          <InputNumber min={1024} max={65535} className="w-full" />
        </Form.Item>
        <Form.Item
          name="minor"
          style={{ display: 'none' }} // Hidden field, always 0 for volume 0
        >
          <InputNumber min={0} />
        </Form.Item>

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

        <Divider orientation="left">Storage Pool Initialization (Optional)</Divider>
        <Form.Item
          name="storage_type"
          label="Storage Type"
          initialValue="none"
          tooltip="Choose storage pool type for selected disks (will wipe data!)"
        >
          <Radio.Group>
            <Radio value="none">None (Use raw disks)</Radio>
            <Radio value="lvm">LVM Storage Pool</Radio>
            <Radio value="zfs">ZFS Storage Pool</Radio>
          </Radio.Group>
        </Form.Item>

        <Form.Item
          noStyle
          shouldUpdate={(prev, current) => prev.storage_type !== current.storage_type}
        >
          {({ getFieldValue }) => {
            const storageType = getFieldValue('storage_type');
            if (storageType === 'none') {
              return null;
            }

            return (
              <>
                {storageType === 'lvm' && (
                  <Row gutter={16}>
                    <Col span={8}>
                      <Form.Item
                        name="lvm_vg_name"
                        label="Volume Group Name"
                        rules={[{ required: true, message: 'VG Name is required' }]}
                      >
                        <Input placeholder="drbd_vg" />
                      </Form.Item>
                    </Col>
                    <Col span={8}>
                      <Form.Item
                        name="lvm_lv_name"
                        label="Logical Volume Name"
                        tooltip="Defaults to resource name"
                      >
                        <Input placeholder="drbd_lv" />
                      </Form.Item>
                    </Col>
                    <Col span={8}>
                      <Form.Item
                        name="lvm_lv_size"
                        label="Size"
                        initialValue="100%FREE"
                        tooltip="e.g. 10G, 100%FREE"
                      >
                        <Input />
                      </Form.Item>
                    </Col>
                  </Row>
                )}

                {storageType === 'zfs' && (
                  <>
                    <Row gutter={16}>
                      <Col span={12}>
                        <Form.Item
                          name="zfs_pool_name"
                          label="ZFS Pool Name"
                          rules={[{ required: true, message: 'Pool name is required' }]}
                          tooltip="Name of the ZFS pool to create"
                        >
                          <Input placeholder="drbd_pool" />
                        </Form.Item>
                      </Col>
                      <Col span={12}>
                        <Form.Item
                          name="zfs_volume_size_gb"
                          label="Volume Size (GB)"
                          initialValue={10}
                          rules={[{ required: true, message: 'Volume size is required' }]}
                          tooltip="Size of the ZFS volume to create"
                        >
                          <InputNumber min={1} className="w-full" />
                        </Form.Item>
                      </Col>
                    </Row>
                    <Form.Item
                      name="zfs_volume_name"
                      label="ZFS Volume Name"
                      tooltip="Defaults to resource name"
                    >
                      <Input placeholder="drbd_volume" />
                    </Form.Item>
                  </>
                )}
              </>
            );
          }}
        </Form.Item>

  
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
