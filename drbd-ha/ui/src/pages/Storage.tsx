import { useState, useEffect } from 'react';
import {
  Table,
  Button,
  Modal,
  Form,
  Input,
  Select,
  Card,
  Space,
  Statistic,
  message,
  Checkbox,
  Row,
  Col,
} from 'antd';
import { PlusOutlined, HddOutlined, ReloadOutlined } from '@ant-design/icons';
import { storageApi, nodesApi } from '@/api';
import type { StoragePool, BlockDevice, Node } from '@/types';

export function Storage() {
  const [pools, setPools] = useState<StoragePool[]>([]);
  const [loading, setLoading] = useState(false);
  const [modalVisible, setModalVisible] = useState(false);
  const [createLoading, setCreateLoading] = useState(false);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [disksByNode, setDisksByNode] = useState<Record<string, BlockDevice[]>>(
    {},
  );
  const [selectedNodes, setSelectedNodes] = useState<string[]>([]);
  const [form] = Form.useForm();

  const fetchPools = async () => {
    setLoading(true);
    try {
      const { pools } = await storageApi.listPools();
      setPools(pools);
    } catch (error) {
      console.error(error);
      message.error('Failed to load storage pools');
    } finally {
      setLoading(false);
    }
  };

  const fetchNodes = async () => {
    try {
      const data = await nodesApi.list();
      setNodes(data);

      // Fetch disks for all nodes
      const disksMap: Record<string, BlockDevice[]> = {};
      for (const node of data) {
        try {
          disksMap[node.id] = await nodesApi.getAvailableDisks(node.id);
        } catch (error) {
          console.error(`Failed to fetch disks for node ${node.id}:`, error);
          disksMap[node.id] = [];
        }
      }
      setDisksByNode(disksMap);
    } catch (error) {
      console.error(error);
    }
  };

  useEffect(() => {
    fetchPools();
    fetchNodes();
  }, []);

  const handleCreate = async (values: any) => {
    if (selectedNodes.length === 0) {
      message.error('Please select at least one node');
      return;
    }

    setCreateLoading(true);
    try {
      const nodeDevices: Record<string, string> = {};
      selectedNodes.forEach((nodeId) => {
        const device = values[`device_${nodeId}`];
        if (device) {
          nodeDevices[nodeId] = device;
        }
      });

      await storageApi.createPool({
        name: values.name,
        pool_type: 'lvm',
        node_devices: nodeDevices,
      });
      message.success('Storage pool created successfully on selected nodes');
      setModalVisible(false);
      form.resetFields();
      setSelectedNodes([]);
      fetchPools();
    } catch (error: any) {
      message.error(
        error.response?.data?.message || 'Failed to create storage pool',
      );
    } finally {
      setCreateLoading(false);
    }
  };

  const columns = [
    {
      title: 'Name',
      dataIndex: 'name',
      key: 'name',
      sorter: (a: StoragePool, b: StoragePool) => a.name.localeCompare(b.name),
      defaultSortOrder: 'ascend',
    },
    {
      title: 'Node',
      dataIndex: 'node_id',
      key: 'node_id',
      render: (nodeId: string) => {
        // Try to find by ID
        let node = nodes.find((n) => n.id === nodeId);
        // If not found, and nodeId === 'local', look for is_local flag
        if (!node && nodeId === 'local') {
          node = nodes.find((n) => n.is_local);
        }
        return node ? node.hostname : nodeId;
      },
    },
    {
      title: 'Device',
      dataIndex: 'device',
      key: 'device',
    },
    {
      title: 'Total Size',
      dataIndex: 'total_size',
      key: 'total_size',
      render: (size: number) => (size / 1024 / 1024 / 1024).toFixed(2) + ' GB',
    },
    {
      title: 'Free Size',
      dataIndex: 'free_size',
      key: 'free_size',
      render: (size: number) => (size / 1024 / 1024 / 1024).toFixed(2) + ' GB',
    },
  ];

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <h2 className="text-xl font-semibold">Storage Pools</h2>
        <Space>
          <Button
            icon={<ReloadOutlined />}
            onClick={fetchPools}
            loading={loading}
          />
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => setModalVisible(true)}
          >
            Create Pool
          </Button>
        </Space>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
        <Card>
          <Statistic
            title="Total Pools"
            value={pools.length}
            prefix={<HddOutlined />}
          />
        </Card>
        <Card>
          <Statistic
            title="Total Capacity"
            value={
              pools.reduce((acc, curr) => acc + curr.total_size, 0) /
              1024 /
              1024 /
              1024
            }
            precision={2}
            suffix="GB"
          />
        </Card>
        <Card>
          <Statistic
            title="Available Free"
            value={
              pools.reduce((acc, curr) => acc + curr.free_size, 0) /
              1024 /
              1024 /
              1024
            }
            precision={2}
            suffix="GB"
            valueStyle={{ color: '#3f8600' }}
          />
        </Card>
      </div>

      <Table
        dataSource={pools}
        columns={columns}
        rowKey="id"
        loading={loading}
      />

      <Modal
        title="Create Storage Pool"
        open={modalVisible}
        onCancel={() => setModalVisible(false)}
        onOk={() => form.submit()}
        confirmLoading={createLoading}
        width={700}
      >
        <Form form={form} layout="vertical" onFinish={handleCreate}>
          <Form.Item
            name="name"
            label="Pool Name"
            rules={[{ required: true, message: 'Please enter pool name' }]}
          >
            <Input placeholder="e.g., ha_pool" />
          </Form.Item>

          <Form.Item label="Create Pool On Nodes">
            <div
              style={{
                border: '1px solid #d9d9d9',
                borderRadius: '4px',
                padding: '12px',
              }}
            >
              {nodes.length === 0 ? (
                <p>No nodes available</p>
              ) : (
                <Row gutter={[16, 16]}>
                  {nodes.map((node) => (
                    <Col span={24} key={node.id}>
                      <div
                        style={{
                          paddingBottom: '12px',
                          borderBottom: '1px solid #f0f0f0',
                        }}
                      >
                        <div style={{ marginBottom: '8px' }}>
                          <Checkbox
                            checked={selectedNodes.includes(node.id)}
                            onChange={(e) => {
                              if (e.target.checked) {
                                setSelectedNodes([...selectedNodes, node.id]);
                              } else {
                                setSelectedNodes(
                                  selectedNodes.filter((id) => id !== node.id),
                                );
                              }
                            }}
                          >
                            <strong>{node.hostname}</strong>{' '}
                            {node.is_local && (
                              <span style={{ color: '#1890ff' }}>(Local)</span>
                            )}
                          </Checkbox>
                        </div>
                        {selectedNodes.includes(node.id) && (
                          <Form.Item
                            name={`device_${node.id}`}
                            label={`Device on ${node.hostname}`}
                            rules={[
                              {
                                required: true,
                                message: 'Please select a device',
                              },
                            ]}
                            style={{ margin: '8px 0 0 24px' }}
                          >
                            <Select placeholder="Select a disk">
                              {(disksByNode[node.id] || []).map((disk) => (
                                <Select.Option
                                  key={disk.path}
                                  value={disk.path}
                                >
                                  {disk.path} ({disk.size_human})
                                </Select.Option>
                              ))}
                            </Select>
                          </Form.Item>
                        )}
                      </div>
                    </Col>
                  ))}
                </Row>
              )}
            </div>
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}
