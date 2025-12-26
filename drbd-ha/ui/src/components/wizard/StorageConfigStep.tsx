import type { FormInstance } from 'antd';
import {
  Card,
  Checkbox,
  Col,
  Collapse,
  Divider,
  Form,
  Input,
  InputNumber,
  Row,
  Radio,
  Select,
  Tooltip,
  Typography,
} from 'antd';
import type { BlockDevice, Node } from '@/types';

const { Text } = Typography;

interface StorageConfigStepProps {
  form: FormInstance;
  nodes: Node[];
  availableDisks: Record<string, BlockDevice[]>;
}

export function StorageConfigStep({
  form,
  nodes,
  availableDisks,
}: StorageConfigStepProps) {
  return (
    <Card title={<span className="text-lg font-semibold">Step 2: Storage Configuration</span>} className="max-w-5xl mx-auto">
      <div className="p-4">
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
                  <>
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
                          label="LV Size"
                          initialValue="100%FREE"
                          tooltip="e.g. 10G, 100%FREE (virtual size for thin volume)"
                        >
                          <Input />
                        </Form.Item>
                      </Col>
                    </Row>
                    <Row gutter={16}>
                      <Col span={12}>
                        <Form.Item
                          name="lvm_thin_pool_name"
                          label="Thin Pool Name"
                          initialValue="thinpool"
                          tooltip="LVM thin pool for efficient storage allocation"
                        >
                          <Input placeholder="thinpool" />
                        </Form.Item>
                      </Col>
                      <Col span={12}>
                        <Form.Item
                          name="lvm_thin_pool_size"
                          label="Thin Pool Metadata Size"
                          initialValue="1G"
                          tooltip="Metadata size for thin pool (1G supports ~6400 volumes)"
                        >
                          <Input placeholder="1G" />
                        </Form.Item>
                      </Col>
                    </Row>
                    <div className="text-sm text-gray-500 mb-4">
                      ℹ️ <strong>Thin provisioning enabled</strong>: Volumes will use only the space they actually need.
                      Thin pool metadata size can be increased later if needed for more volumes.
                    </div>
                  </>
                )}

                {storageType === 'zfs' && (
                  <>
                    <Form.Item
                      name="zfs_thin_volume"
                      initialValue={true}
                      valuePropName="checked"
                      hidden
                    >
                      <Checkbox />
                    </Form.Item>
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
                          label="Virtual Volume Size (GB)"
                          initialValue={10}
                          rules={[{ required: true, message: 'Volume size is required' }]}
                          tooltip="Virtual size (actual space used depends on data)"
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
                    <div className="text-sm text-gray-500 mb-4">
                      ℹ️ <strong>Thin provisioning enabled</strong>: ZFS sparse volumes allocate space on-demand.
                      The volume size is virtual; actual disk usage will grow as data is written.
                    </div>
                  </>
                )}
              </>
            );
          }}
        </Form.Item>

        <Divider orientation="left">DRBD Advanced Options</Divider>
        <Collapse
          items={[
            {
              key: 'drbd-advanced',
              label: <span className="font-semibold">DRBD Network & Split-Brain Policies</span>,
              children: (
                <div className="space-y-4">
                  <div className="bg-blue-50 p-3 rounded border border-blue-200">
                    <Text type="secondary" className="text-xs">
                      ℹ️ These options control DRBD behavior during network partitions and split-brain scenarios.
                      For two-node clusters, <strong>preferred-nodes</strong> in HA Config is typically sufficient.
                      These policies provide additional automatic recovery mechanisms.
                    </Text>
                  </div>

                  <Row gutter={16}>
                    <Col span={8}>
                      <Form.Item
                        name="protocol"
                        label="Replication Protocol"
                        initialValue="C"
                        tooltip={
                          <span>
                            <strong>A:</strong> Async (fastest, risk of data loss)<br/>
                            <strong>B:</strong> Semi-sync (balanced)<br/>
                            <strong>C:</strong> Sync (safest, recommended)
                          </span>
                        }
                      >
                        <Select>
                          <Select.Option value="A">A (Async)</Select.Option>
                          <Select.Option value="B">B (Semi-sync)</Select.Option>
                          <Select.Option value="C">C (Sync)</Select.Option>
                        </Select>
                      </Form.Item>
                    </Col>

                    <Col span={8}>
                      <Form.Item
                        name="verify_alg"
                        label="Data Integrity"
                        initialValue="sha256"
                        tooltip="Algorithm for verifying data integrity during replication"
                      >
                        <Select>
                          <Select.Option value="none">None</Select.Option>
                          <Select.Option value="crc32c">CRC32C (Fast)</Select.Option>
                          <Select.Option value="sha1">SHA1</Select.Option>
                          <Select.Option value="sha256">SHA256 (Secure)</Select.Option>
                        </Select>
                      </Form.Item>
                    </Col>

                    <Col span={8}>
                      <Form.Item
                        name="max_epoch_size"
                        label="Max Epoch Size"
                        initialValue={2048}
                        tooltip="Maximum number of write requests before barrier"
                      >
                        <InputNumber min={1} max={20000} className="w-full" />
                      </Form.Item>
                    </Col>
                  </Row>

                  <Divider orientation="left" className="text-xs">Split-Brain Recovery Policies</Divider>

                  <div className="bg-yellow-50 p-3 rounded border border-yellow-200 mb-4">
                    <Text type="secondary" className="text-xs">
                      ⚠️ <strong>Split-brain</strong> occurs when both nodes become Primary due to network partition.
                      These policies define automatic recovery. For manual recovery, use the "Recover Split-Brain" action.
                    </Text>
                  </div>

                  <Row gutter={16}>
                    <Col span={24}>
                      <Form.Item
                        name="after_sb_0pri"
                        label="After-SB-0Pri (Both Secondary)"
                        initialValue="disconnect"
                        tooltip="Policy when both nodes are Secondary after split-brain"
                      >
                        <Select>
                          <Select.Option value="disconnect">
                            disconnect (Manual recovery required)
                          </Select.Option>
                          <Select.Option value="discard-zero-changes">
                            discard-zero-changes (Auto-discard unchanged data)
                          </Select.Option>
                          <Select.Option value="call-pri-lost-after-sb">
                            call-pri-lost-after-sb (Call recovery script)
                          </Select.Option>
                        </Select>
                      </Form.Item>
                    </Col>

                    <Col span={24}>
                      <Form.Item
                        name="after_sb_1pri"
                        label="After-SB-1Pri (One Primary)"
                        initialValue="disconnect"
                        tooltip="Policy when one node is Primary after split-brain"
                      >
                        <Select>
                          <Select.Option value="disconnect">
                            disconnect (Manual recovery required)
                          </Select.Option>
                          <Select.Option value="consensus">
                            consensus (Requires both nodes to agree)
                          </Select.Option>
                          <Select.Option value="call-pri-lost-after-sb">
                            call-pri-lost-after-sb (Call recovery script)
                          </Select.Option>
                          <Select.Option value="violently-as-0pri">
                            violently-as-0pri (Force both to Secondary)
                          </Select.Option>
                        </Select>
                      </Form.Item>
                    </Col>

                    <Col span={24}>
                      <Form.Item
                        name="after_sb_2pri"
                        label="After-SB-2Pri (Both Primary)"
                        initialValue="disconnect"
                        tooltip="Policy when both nodes are Primary after split-brain (dangerous!)"
                      >
                        <Select>
                          <Select.Option value="disconnect">
                            disconnect (Safest - Manual recovery)
                          </Select.Option>
                          <Select.Option value="violently-as-0pri">
                            violently-as-0pri (Force both to Secondary - DATA LOSS RISK)
                          </Select.Option>
                          <Select.Option value="call-pri-lost-after-sb">
                            call-pri-lost-after-sb (Call recovery script)
                          </Select.Option>
                        </Select>
                      </Form.Item>
                    </Col>

                    {/* Removed rs-discard-granularity and data-integrity-alg as they are rarely used */}
                  </Row>
                </div>
              ),
            },
          ]}
        />

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
      </div>
    </Card>
  );
}
