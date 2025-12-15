import { DeleteOutlined, DownOutlined, PlusOutlined, ReloadOutlined } from '@ant-design/icons';
import {
  Button,
  Dropdown,
  Form,
  Input,
  InputNumber,
  Modal,
  message,
  Popconfirm,
  Select,
  Space,
  Table,
  Tag,
} from 'antd';
import { useEffect, useState } from 'react';
import { nodesApi, resourcesApi } from '@/api';
import { useNodesStore } from '@/stores/nodes';
import { useResourcesStore } from '@/stores/resources';
import type { BlockDevice, CreateResourceRequest, DrbdResource } from '@/types';

const roleColor: Record<string, string> = {
  Primary: 'green',
  Secondary: 'blue',
  Unknown: 'default',
};

const diskStateColor: Record<string, string> = {
  UpToDate: 'green',
  Inconsistent: 'orange',
  Diskless: 'red',
  DUnknown: 'default',
};

export function Resources() {
  const { resources, loading, fetch } = useResourcesStore();
  const { nodes, fetch: fetchNodes } = useNodesStore();
  const [modalOpen, setModalOpen] = useState(false);
  const [form] = Form.useForm<CreateResourceRequest>();
  const [submitting, setSubmitting] = useState(false);
  const [availableDisks, setAvailableDisks] = useState<
    Record<string, BlockDevice[]>
  >({});

  useEffect(() => {
    fetch();
    fetchNodes();
  }, [fetch, fetchNodes]);

  useEffect(() => {
    // Load available disks for each node - only fetch if not already loaded
    nodes.forEach(async (node) => {
      if (!availableDisks[node.id]) {
        try {
          const disks = await nodesApi.getAvailableDisks(node.id);
          setAvailableDisks((prev) => ({ ...prev, [node.id]: disks }));
        } catch {}
      }
    });
  }, [nodes, availableDisks]);

  const handleCreate = async (values: CreateResourceRequest) => {
    setSubmitting(true);
    try {
      await resourcesApi.create(values);
      message.success('Resource created');
      setModalOpen(false);
      form.resetFields();
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      setSubmitting(false);
    }
  };

  const handleAction = async (name: string, action: string, force = false) => {
    try {
      const result = await resourcesApi.action(name, {
        action: action as any,
        force,
      });
      if (result.success) {
        message.success(`${action} completed`);
      } else {
        message.error(result.message || `${action} failed`);
      }
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleDelete = async (name: string) => {
    try {
      await resourcesApi.delete(name);
      message.success('Resource deleted');
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleInit = async (name: string) => {
    try {
      await resourcesApi.init(name);
      message.success('Resource initialized');
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const refreshAvailableDisks = async () => {
    const disksMap: Record<string, BlockDevice[]> = {};
    for (const node of nodes) {
      try {
        const disks = await nodesApi.getAvailableDisks(node.id);
        disksMap[node.id] = disks;
      } catch {}
    }
    setAvailableDisks(disksMap);
    message.success('Available disks refreshed');
  };

  const getActionItems = (record: DrbdResource) => [
    { key: 'up', label: 'Up', onClick: () => handleAction(record.name, 'up') },
    {
      key: 'down',
      label: 'Down',
      onClick: () => handleAction(record.name, 'down'),
    },
    { type: 'divider' as const },
    {
      key: 'primary',
      label: 'Primary',
      onClick: () => handleAction(record.name, 'primary'),
    },
    {
      key: 'primary-force',
      label: 'Primary (Force)',
      onClick: () => handleAction(record.name, 'primary', true),
    },
    {
      key: 'secondary',
      label: 'Secondary',
      onClick: () => handleAction(record.name, 'secondary'),
    },
    { type: 'divider' as const },
    {
      key: 'init',
      label: 'Initialize',
      onClick: () => handleInit(record.name),
    },
  ];

  const columns = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    {
      title: 'Role',
      dataIndex: 'role',
      key: 'role',
      render: (role: string) => (
        <Tag color={roleColor[role] || 'default'}>{role}</Tag>
      ),
    },
    {
      title: 'Disk State',
      key: 'disk_state',
      render: (_: unknown, record: DrbdResource) => {
        const state = record.devices[0]?.disk_state || 'Unknown';
        return <Tag color={diskStateColor[state] || 'default'}>{state}</Tag>;
      },
    },
    {
      title: 'Connections',
      key: 'connections',
      render: (_: unknown, record: DrbdResource) => (
        <Space>
          {record.connections.map((c) => (
            <Tag
              key={c.name}
              color={c.connection_state === 'Connected' ? 'green' : 'orange'}
            >
              {c.name}: {c.connection_state}
            </Tag>
          ))}
        </Space>
      ),
    },
    {
      title: 'Actions',
      key: 'actions',
      render: (_: unknown, record: DrbdResource) => (
        <Space>
          <Dropdown menu={{ items: getActionItems(record) }}>
            <Button size="small">
              Actions <DownOutlined />
            </Button>
          </Dropdown>
          <Popconfirm
            title="Delete this resource?"
            onConfirm={() => handleDelete(record.name)}
          >
            <Button size="small" danger icon={<DeleteOutlined />} />
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h2 className="text-2xl font-semibold">DRBD Resources</h2>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={refreshAvailableDisks}
          >
            Refresh Disks
          </Button>
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => setModalOpen(true)}
          >
            Create Resource
          </Button>
        </Space>
      </div>

      <Table
        dataSource={resources}
        columns={columns}
        rowKey="name"
        loading={loading}
        pagination={false}
      />

      <Modal
        title="Create DRBD Resource"
        open={modalOpen}
        onCancel={() => setModalOpen(false)}
        footer={null}
        width={600}
        destroyOnClose
      >
        <Form form={form} layout="vertical" onFinish={handleCreate}>
          <Form.Item
            name="name"
            label="Resource Name"
            rules={[{ required: true, pattern: /^[a-zA-Z][a-zA-Z0-9_-]*$/ }]}
          >
            <Input placeholder="r0" />
          </Form.Item>
          <Form.Item
            name="port"
            label="DRBD Port"
            rules={[{ required: true }]}
            initialValue={7789}
          >
            <InputNumber min={7000} max={8000} className="w-full" />
          </Form.Item>
          <Form.Item
            name="minor"
            label="Minor Number"
            rules={[{ required: true }]}
            initialValue={0}
          >
            <InputNumber min={0} className="w-full" />
          </Form.Item>
          <Form.Item
            name="auto_promote"
            label="Auto Promote"
            initialValue={false}
          >
            <Select
              options={[
                { value: true, label: 'Yes (Standard DRBD)' },
                { value: false, label: 'No (For HA/drbd-reactor)' },
              ]}
            />
          </Form.Item>
          <Form.Item label="Node Disks" required>
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
          </Form.Item>
          <Form.Item>
            <Button type="primary" htmlType="submit" loading={submitting} block>
              Create Resource
            </Button>
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
