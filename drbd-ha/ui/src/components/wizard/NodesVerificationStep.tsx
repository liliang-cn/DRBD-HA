import { useState } from 'react';
import {
  Card,
  Table,
  Tag,
  Alert,
  Button,
  Modal,
  Form,
  Input,
  InputNumber,
  Space,
  message,
  Popconfirm,
} from 'antd';
import {
  PlusOutlined,
  ReloadOutlined,
  DeleteOutlined,
} from '@ant-design/icons';
import { useNodesStore } from '@/stores/nodes';
import { nodesApi } from '@/api';
import type { Node, AddNodeRequest } from '@/types';

const statusColor: Record<string, string> = {
  online: 'green',
  offline: 'red',
  error: 'orange',
  unknown: 'default',
};

interface NodesVerificationStepProps {
  nodes: Node[];
}

export function NodesVerificationStep({ nodes }: NodesVerificationStepProps) {
  const { add, remove, fetch } = useNodesStore();
  const [modalOpen, setModalOpen] = useState(false);
  const [form] = Form.useForm<AddNodeRequest>();
  const [submitting, setSubmitting] = useState(false);

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

  const columns = [
    { title: 'Hostname', dataIndex: 'hostname', key: 'hostname' },
    { title: 'IP', dataIndex: 'ip', key: 'ip' },
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
          <span>Step 1: Select or Add Cluster Nodes</span>
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => setModalOpen(true)}
          >
            Add Node
          </Button>
        </div>
      }
      className="max-w-4xl mx-auto"
    >
      <Table
        dataSource={nodes}
        columns={columns}
        rowKey="id"
        pagination={false}
      />

      {nodes.length < 2 && (
        <Alert
          message="At least 2 nodes are required for HA"
          type="warning"
          showIcon
          className="mt-4"
        />
      )}

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
    </Card>
  );
}