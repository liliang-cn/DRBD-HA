import {
  DeleteOutlined,
  PlusOutlined,
  ReloadOutlined,
  EditOutlined,
} from '@ant-design/icons';
import {
  Alert,
  Button,
  Card,
  Form,
  Input,
  InputNumber,
  Modal,
  message,
  Popconfirm,
  Space,
  Table,
  Tag,
} from 'antd';
import { useEffect, useState } from 'react';
import { nodesApi } from '@/api';
import { useNodesStore } from '@/stores/nodes';
import { useThemeStore } from '@/stores/theme';
import type { AddNodeRequest, Node } from '@/types';
import type { WizardSharedState } from './types';

const statusColor: Record<string, string> = {
  online: 'green',
  offline: 'red',
  error: 'orange',
  unknown: 'default',
};

interface NodesVerificationStepProps {
  nodes: Node[];
  sharedState: WizardSharedState;
}

export function NodesVerificationStep({
  nodes,
  sharedState,
}: NodesVerificationStepProps) {
  const { add, remove, fetch, update } = useNodesStore();
  const { theme: currentTheme } = useThemeStore();
  const [modalOpen, setModalOpen] = useState(false);
  const [editModalOpen, setEditModalOpen] = useState(false);
  const [editingNode, setEditingNode] = useState<Node | null>(null);
  const [form] = Form.useForm<AddNodeRequest>();
  const [submitting, setSubmitting] = useState(false);
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);

  // Initialize selected nodes with all nodes by default
  useEffect(() => {
    if (nodes.length > 0 && selectedRowKeys.length === 0) {
      const allKeys = nodes.map((n) => n.id);
      setSelectedRowKeys(allKeys);
      sharedState.setSelectedNodes(nodes);
    }
  }, [nodes, selectedRowKeys.length, sharedState.setSelectedNodes]);

  const handleSelectionChange = (newSelectedRowKeys: React.Key[]) => {
    setSelectedRowKeys(newSelectedRowKeys);
    const selectedNodes = nodes.filter((n) =>
      newSelectedRowKeys.includes(n.id),
    );
    sharedState.setSelectedNodes(selectedNodes);
  };

  const handleAdd = async (values: AddNodeRequest) => {
    setSubmitting(true);
    try {
      await add(values);
      message.success('Node added successfully');
      setModalOpen(false);
      form.resetFields();
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await remove(id);
      message.success('Node removed');
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleCheck = async (id: string) => {
    try {
      const result = await nodesApi.check(id);
      if (result.status === 'online') {
        message.success(`Node ${result.hostname} is online`);
      } else {
        message.warning(
          `Node ${result.hostname}: ${result.message || result.status}`,
        );
      }
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleEdit = (node: Node) => {
    setEditingNode(node);
    form.setFieldsValue({
      hostname: node.hostname,
      ip: node.ip,
      ssh_port: node.ssh_port,
      ssh_user: node.ssh_user || '',
    });
    setEditModalOpen(true);
  };

  const handleUpdate = async (values: AddNodeRequest) => {
    if (!editingNode) return;
    setSubmitting(true);
    try {
      await update(editingNode.id, values);
      message.success('Node updated successfully');
      setEditModalOpen(false);
      setEditingNode(null);
      form.resetFields();
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      setSubmitting(false);
    }
  };

  const columns = [
    { title: 'Hostname', dataIndex: 'hostname', key: 'hostname' },
    { title: 'IP', dataIndex: 'ip', key: 'ip' },
    {
      title: 'SSH User',
      dataIndex: 'ssh_user',
      key: 'ssh_user',
      render: (sshUser: string, record: Node) => (
        <Space>
          <span>{sshUser || <Tag color="red">Not set</Tag>}</span>
          {!sshUser && (
            <Button
              type="link"
              size="small"
              icon={<EditOutlined />}
              onClick={() => handleEdit(record)}
            >
              Set
            </Button>
          )}
        </Space>
      ),
    },
    {
      title: 'Status',
      dataIndex: 'status',
      render: (status: string) => (
        <Tag color={statusColor[status]}>{status.toUpperCase()}</Tag>
      ),
    },
    {
      title: 'Type',
      render: (_, r: { is_local: boolean }) => (
        <Tag>{r.is_local ? 'Local' : 'Remote'}</Tag>
      ),
    },
    {
      title: 'Actions',
      key: 'actions',
      render: (_: unknown, record: Node) => (
        <Space>
          <Button
            size="small"
            icon={<ReloadOutlined />}
            onClick={() => handleCheck(record.id)}
          >
            Check
          </Button>
          <Button
            size="small"
            icon={<EditOutlined />}
            onClick={() => handleEdit(record)}
          >
            Edit
          </Button>
          {!record.is_local && (
            <Popconfirm
              title="Delete this node?"
              onConfirm={() => handleDelete(record.id)}
            >
              <Button size="small" danger icon={<DeleteOutlined />}>
                Delete
              </Button>
            </Popconfirm>
          )}
        </Space>
      ),
    },
  ];

  return (
    <Card
      title={
        <div className="flex justify-between items-center">
          <span className="text-lg font-semibold">
            Step 1: Select or Add Cluster Nodes
          </span>
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => setModalOpen(true)}
          >
            Add Node
          </Button>
        </div>
      }
      className="w-full"
    >
      <div className="p-4">
        {nodes.length === 0 ? (
          <Alert
            message="No nodes available"
            description={
              <div>
                <p>Please add at least 2 nodes to configure HA.</p>
                <Button
                  type="primary"
                  icon={<PlusOutlined />}
                  onClick={() => setModalOpen(true)}
                  className="mt-2"
                >
                  Add Node
                </Button>
              </div>
            }
            type="info"
            showIcon
          />
        ) : (
          <>
            <Table
              dataSource={nodes}
              columns={columns}
              rowKey="id"
              pagination={false}
              rowSelection={{
                selectedRowKeys,
                onChange: handleSelectionChange,
                getCheckboxProps: (record: Node) => ({
                  // Disable checkbox for nodes that are offline
                  disabled: record.status !== 'online',
                  name: record.hostname,
                }),
              }}
            />

            {selectedRowKeys.length < 2 && (
              <Alert
                message="At least 2 nodes must be selected for HA"
                type="warning"
                showIcon
                className="mt-6"
              />
            )}
          </>
        )}
      </div>

      <Modal
        title="Add Cluster Node"
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        footer={null}
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={handleAdd}>
          <Form.Item
            name="hostname"
            label="Hostname"
            rules={[{ required: true }]}
          >
            <Input placeholder="node2" />
          </Form.Item>
          <Form.Item name="ip" label="IP Address" rules={[{ required: true }]}>
            <Input placeholder="192.168.1.102" />
          </Form.Item>
          <Form.Item name="ssh_port" label="SSH Port" initialValue={22}>
            <InputNumber min={1} max={65535} className="w-full" />
          </Form.Item>
          <Form.Item name="ssh_user" label="SSH User" initialValue="root">
            <Input />
          </Form.Item>
          <Form.Item>
            <Button type="primary" htmlType="submit" loading={submitting} block>
              Add Node
            </Button>
          </Form.Item>
        </Form>
      </Modal>

      <Modal
        title="Edit Node"
        open={editModalOpen}
        onCancel={() => {
          setEditModalOpen(false);
          setEditingNode(null);
          form.resetFields();
        }}
        footer={null}
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={handleUpdate}>
          <Form.Item
            name="hostname"
            label="Hostname"
            rules={[{ required: true }]}
          >
            <Input disabled />
          </Form.Item>
          <Form.Item name="ip" label="IP Address" rules={[{ required: true }]}>
            <Input />
          </Form.Item>
          <Form.Item name="ssh_port" label="SSH Port" initialValue={22}>
            <InputNumber min={1} max={65535} className="w-full" />
          </Form.Item>
          <Form.Item
            name="ssh_user"
            label="SSH User"
            rules={[{ required: true, message: 'SSH User is required for remote operations' }]}
          >
            <Input placeholder="root" />
          </Form.Item>
          <Form.Item>
            <Button type="primary" htmlType="submit" loading={submitting} block>
              Update Node
            </Button>
          </Form.Item>
        </Form>
      </Modal>
    </Card>
  );
}
