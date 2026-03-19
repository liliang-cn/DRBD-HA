import {
  DeleteOutlined,
  PlusOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import {
  Alert,
  Button,
  Form,
  Input,
  InputNumber,
  Modal,
  message,
  Popconfirm,
  Space,
  Table,
  Tag,
  Tooltip,
} from 'antd';
import { useEffect, useState } from 'react';
import { nodesApi } from '@/api';
import { useNodesStore } from '@/stores/nodes';
import type { AddNodeRequest, Node } from '@/types';

const statusColor: Record<string, string> = {
  online: 'green',
  offline: 'red',
  error: 'orange',
  unknown: 'default',
};

export function Nodes() {
  const { nodes, loading, fetch, add, remove } = useNodesStore();
  const [modalOpen, setModalOpen] = useState(false);
  const [form] = Form.useForm<AddNodeRequest>();
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    fetch();
  }, [fetch]);

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
      } else if (result.status === 'offline') {
        message.error(
          `Node ${result.hostname}: ${result.message || 'SSH connection failed'}`,
        );
      } else {
        message.error(
          `Node ${result.hostname}: ${result.message || result.status}`,
        );
      }
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const columns = [
    { title: 'Hostname', dataIndex: 'hostname', key: 'hostname' },
    { title: 'IP', dataIndex: 'ip', key: 'ip' },
    { title: 'SSH Port', dataIndex: 'ssh_port', key: 'ssh_port' },
    { title: 'User', dataIndex: 'ssh_user', key: 'ssh_user' },
    {
      title: 'Status',
      dataIndex: 'status',
      key: 'status',
      render: (status: Node['status'], record: Node) => {
        const tag = (
          <Tag color={statusColor[status]}>{status.toUpperCase()}</Tag>
        );
        return record.status_message ? (
          <Tooltip title={record.status_message}>{tag}</Tooltip>
        ) : (
          tag
        );
      },
    },
    {
      title: 'Type',
      key: 'type',
      render: (_: unknown, record: Node) => (
        <Tag>{record.is_local ? 'Local' : 'Remote'}</Tag>
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
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h2 className="text-2xl font-semibold">Nodes</h2>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => setModalOpen(true)}
        >
          Add Node
        </Button>
      </div>

      <Alert
        type="info"
        showIcon
        message="Remote access requirements"
        description="Nodes must allow passwordless SSH. If SSH User is not root, that user must also support passwordless sudo (`sudo -n`). Node Check only reports online when these requirements pass."
      />

      <Table
        dataSource={nodes}
        columns={columns}
        rowKey="id"
        loading={loading}
        pagination={false}
      />

      <Modal
        title="Add Node"
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
          <Form.Item
            name="ssh_user"
            label="SSH User"
            initialValue="root"
            extra="Use root, or a user with passwordless sudo (`sudo -n`)."
          >
            <Input />
          </Form.Item>
          <Form.Item>
            <Button type="primary" htmlType="submit" loading={submitting} block>
              Add Node
            </Button>
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
