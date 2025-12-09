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
  Alert,
} from "antd";
import type { FormInstance } from "antd";
import { HddOutlined, DatabaseOutlined } from "@ant-design/icons";
import type { Node, BlockDevice, StoragePool } from "@/types";

interface StorageConfigStepProps {
  form: FormInstance;
  storageStrategy: "raw" | "lvm";
  onStrategyChange: (strategy: "raw" | "lvm") => void;
  nodes: Node[];
  availableDisks: Record<string, BlockDevice[]>;
  storagePools: StoragePool[];
}

export function StorageConfigStep({
  form,
  storageStrategy,
  onStrategyChange,
  nodes,
  availableDisks,
  storagePools,
}: StorageConfigStepProps) {
  return (
    <Card title="Step 2: Storage Configuration" className="max-w-4xl mx-auto">
      <Form form={form} layout="vertical">
        <Form.Item label="Storage Strategy">
          <Radio.Group
            value={storageStrategy}
            onChange={(e) => onStrategyChange(e.target.value)}
            buttonStyle="solid"
          >
            <Radio.Button value="raw">
              <HddOutlined /> Raw Disk (Manual)
            </Radio.Button>
            <Radio.Button value="lvm">
              <DatabaseOutlined /> LVM Storage Pool (Automatic)
            </Radio.Button>
          </Radio.Group>
        </Form.Item>

        <Divider />

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
              initialValue={7789}
            >
              <InputNumber min={7000} max={8000} className="w-full" />
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

        {storageStrategy === "raw" ? (
          <>
            <Form.Item
              name="fs_type"
              label="Filesystem Type"
              initialValue="xfs"
            >
              <Select
                options={[
                  { value: "xfs" },
                  { value: "ext4" },
                  { value: "btrfs" },
                ]}
              />
            </Form.Item>
            <Divider>Node Disks</Divider>
            {nodes.map((node) => (
              <Form.Item
                key={node.id}
                name={["node_disks", node.id]}
                label={`${node.hostname} (${node.ip})`}
                rules={[{ required: true, message: "Select a disk" }]}
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
          </>
        ) : (
          <>
            <Row gutter={16}>
              <Col span={12}>
                <Form.Item
                  name="lvm_pool_id"
                  label="Storage Pool"
                  rules={[{ required: true }]}
                >
                  <Select
                    placeholder="Select LVM Pool"
                    options={storagePools.map((p) => ({
                      value: p.id,
                      label: `${p.name} (Free: ${(
                        p.free_size /
                        1024 /
                        1024 /
                        1024
                      ).toFixed(1)} GB)`,
                    }))}
                  />
                </Form.Item>
              </Col>
              <Col span={12}>
                <Form.Item
                  name="lvm_volume_size_gb"
                  label="Volume Size (GB)"
                  rules={[{ required: true }]}
                  initialValue={10}
                >
                  <InputNumber min={1} className="w-full" />
                </Form.Item>
              </Col>
            </Row>
            <Form.Item
              name="fs_type"
              label="Filesystem Type"
              initialValue="xfs"
            >
              <Select
                options={[
                  { value: "xfs" },
                  { value: "ext4" },
                  { value: "btrfs" },
                ]}
              />
            </Form.Item>
            <Alert
              message="LVM volumes will be automatically created on all nodes using the selected pool."
              type="info"
              showIcon
            />
          </>
        )}
      </Form>
    </Card>
  );
}
