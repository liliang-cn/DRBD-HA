import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  Table,
  Button,
  Tag,
  Modal,
  Form,
  Input,
  InputNumber,
  message,
  Space,
  Card,
  Descriptions,
  Dropdown,
  Checkbox,
} from 'antd';
import {
  PlusOutlined,
  DeleteOutlined,
  DownOutlined,
  EyeOutlined,
  LoadingOutlined,
  ExclamationCircleOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  SearchOutlined,
  AppstoreOutlined,
  CloudServerOutlined,
} from '@ant-design/icons';
import { useHaProfilesStore } from '@/stores/ha-profiles';
import { useResourcesStore } from '@/stores/resources';
import { haProfilesApi } from '@/api';
import { ImportProfilesModal } from '@/components/ha/ImportProfilesModal';
import type { HaProfile, HaProfileStatus, VipConfig } from '@/types';

const statusColor: Record<string, string> = {
  active: 'green',
  standby: 'blue',
  stopped: 'default',
  error: 'red',
  unknown: 'default',
};

export function HaProfiles() {
  const navigate = useNavigate();
  const { profiles, loading, statusLoading, fetch } = useHaProfilesStore();
  const { fetch: fetchResources } = useResourcesStore();
  const [statusModalOpen, setStatusModalOpen] = useState(false);
  const [selectedStatus, setSelectedStatus] = useState<HaProfileStatus | null>(
    null,
  );
  const [selectedProfile, setSelectedProfile] = useState<HaProfile | null>(
    null,
  );
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [profileToDelete, setProfileToDelete] = useState<HaProfile | null>(
    null,
  );
  const [deleteResource, setDeleteResource] = useState(true);
  const [deleting, setDeleting] = useState(false);
  const [vipModalOpen, setVipModalOpen] = useState(false);
  const [vipForm] = Form.useForm<VipConfig>();
  const [vipSubmitting, setVipSubmitting] = useState(false);
  const [selectedProfileForVip, setSelectedProfileForVip] =
    useState<HaProfile | null>(null);
  const [importModalOpen, setImportModalOpen] = useState(false);

  useEffect(() => {
    fetch();
    fetchResources();
  }, [fetch, fetchResources]);

  const openDeleteModal = (profile: HaProfile) => {
    setProfileToDelete(profile);
    setDeleteResource(true);
    setDeleteModalOpen(true);
  };

  const handleDelete = async () => {
    if (!profileToDelete) return;
    setDeleting(true);
    try {
      await haProfilesApi.delete(profileToDelete.id, deleteResource);
      message.success(
        deleteResource
          ? 'HA Profile and DRBD resource deleted'
          : 'HA Profile deleted',
      );
      setDeleteModalOpen(false);
      setProfileToDelete(null);
      fetch();
      fetchResources();
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      setDeleting(false);
    }
  };

  const handleViewStatus = async (profile: HaProfile) => {
    setSelectedProfile(profile);
    try {
      const status = await haProfilesApi.getStatus(profile.id);
      setSelectedStatus(status);
      setStatusModalOpen(true);
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleActivate = async (id: string) => {
    try {
      await haProfilesApi.activate(id);
      message.success('Profile activated');
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleDeactivate = async (id: string) => {
    try {
      await haProfilesApi.deactivate(id);
      message.success('Profile deactivated');
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleEvict = async (id: string) => {
    try {
      await haProfilesApi.evict(id);
      message.success('Eviction initiated');
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleReloadReactor = async () => {
    try {
      await haProfilesApi.reloadReactor();
      message.success('drbd-reactor reloaded');
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const openAddVipModal = (profile: HaProfile) => {
    setSelectedProfileForVip(profile);
    vipForm.resetFields();
    vipForm.setFieldsValue({ netmask: 24 });
    setVipModalOpen(true);
  };

  const handleAddVip = async (values: VipConfig) => {
    if (!selectedProfileForVip) return;
    setVipSubmitting(true);
    try {
      await haProfilesApi.addVip(selectedProfileForVip.id, values);
      message.success('VIP added successfully');
      setVipModalOpen(false);
      setSelectedProfileForVip(null);
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      setVipSubmitting(false);
    }
  };

  const handleRemoveVip = async (profile: HaProfile) => {
    try {
      await haProfilesApi.removeVip(profile.id);
      message.success('VIP removed successfully');
      fetch();
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const getActionItems = (record: HaProfile) => {
    const items: Array<{
      key?: string;
      label?: string;
      icon?: React.ReactNode;
      onClick?: () => void;
      type?: 'divider';
      danger?: boolean;
    }> = [
      {
        key: 'status',
        label: 'View Status',
        icon: <EyeOutlined />,
        onClick: () => handleViewStatus(record),
      },
    ];

    const isActive = record.status === 'active';
    const hasVip = !!record.vip;

    // Show Activate only when not active
    if (!isActive) {
      items.push({ type: 'divider' as const });
      items.push({
        key: 'activate',
        label: 'Activate',
        onClick: () => handleActivate(record.id),
      });
    }

    // Show Deactivate only when active
    if (isActive) {
      items.push({ type: 'divider' as const });
      items.push({
        key: 'deactivate',
        label: 'Deactivate',
        onClick: () => handleDeactivate(record.id),
      });
      items.push({
        key: 'evict',
        label: 'Evict (Failover)',
        danger: true,
        onClick: () => handleEvict(record.id),
      });
    }

    // VIP management
    items.push({ type: 'divider' as const });
    if (hasVip) {
      items.push({
        key: 'remove-vip',
        label: 'Remove VIP',
        danger: true,
        onClick: () => handleRemoveVip(record),
      });
    } else {
      items.push({
        key: 'add-vip',
        label: 'Add VIP',
        onClick: () => openAddVipModal(record),
      });
    }

    return items;
  };

  const columns = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    {
      title: 'Type',
      dataIndex: 'ha_type',
      key: 'ha_type',
      render: (t: string) => <Tag>{(t || 'generic').toUpperCase()}</Tag>,
    },
    { title: 'Resource', dataIndex: 'resource_name', key: 'resource_name' },
    { title: 'Mount Point', dataIndex: 'mount_point', key: 'mount_point' },
    {
      title: 'VIP',
      key: 'vip',
      render: (_: unknown, record: HaProfile) =>
        record.vip ? (
          <Space size="small">
            <Tag color="green" icon={<CheckCircleOutlined />}>
              Enabled
            </Tag>
            <span className="text-gray-500 text-xs">{record.vip.address}</span>
          </Space>
        ) : (
          <Tag color="default" icon={<CloseCircleOutlined />}>
            Disabled
          </Tag>
        ),
    },
    {
      title: 'Status',
      dataIndex: 'status',
      key: 'status',
      render: (status: string, record: HaProfile) => {
        const isLoading = statusLoading.has(record.id);
        if (isLoading) {
          return (
            <Tag color="default">
              <LoadingOutlined spin className="mr-1" />
              Loading...
            </Tag>
          );
        }
        return (
          <Tag color={statusColor[status] || 'default'}>
            {status.toUpperCase()}
          </Tag>
        );
      },
    },
    {
      title: 'Services',
      key: 'services',
      render: (_: unknown, record: HaProfile) => (
        <Space>
          {record.promoter.services.slice(0, 2).map((s) => (
            <Tag key={s}>{s}</Tag>
          ))}
          {record.promoter.services.length > 2 && (
            <Tag>+{record.promoter.services.length - 2} more</Tag>
          )}
        </Space>
      ),
    },
    {
      title: 'Actions',
      key: 'actions',
      render: (_: unknown, record: HaProfile) => (
        <Space>
          <Dropdown menu={{ items: getActionItems(record) }}>
            <Button size="small">
              Actions <DownOutlined />
            </Button>
          </Dropdown>
          <Button
            size="small"
            danger
            icon={<DeleteOutlined />}
            onClick={() => openDeleteModal(record)}
          />
        </Space>
      ),
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h2 className="text-2xl font-semibold">HA Profiles</h2>
        <Space>
          <Button onClick={handleReloadReactor}>Reload drbd-reactor</Button>
          <Button
            icon={<SearchOutlined />}
            onClick={() => setImportModalOpen(true)}
          >
            Discover / Import
          </Button>
          <Dropdown
            menu={{
              items: [
                {
                  key: 'service',
                  label: 'Service HA',
                  icon: <AppstoreOutlined />,
                  onClick: () => navigate('/service-ha/create'),
                },
                {
                  key: 'storage',
                  label: 'Storage Sharing',
                  icon: <CloudServerOutlined />,
                  onClick: () => navigate('/storage-sharing/create'),
                },
              ],
            }}
          >
            <Button type="primary" icon={<PlusOutlined />}>
              Create <DownOutlined />
            </Button>
          </Dropdown>
        </Space>
      </div>

      <Table
        dataSource={profiles}
        columns={columns}
        rowKey="id"
        loading={loading}
        pagination={false}
      />

      <ImportProfilesModal
        open={importModalOpen}
        onCancel={() => setImportModalOpen(false)}
        onSuccess={() => {
          fetch();
          fetchResources();
        }}
      />

      {/* Status Modal */}
      <Modal
        title="HA Profile Status"
        open={statusModalOpen}
        onCancel={() => setStatusModalOpen(false)}
        footer={null}
        width={800}
      >
        {selectedStatus && selectedProfile && (
          <div className="space-y-4">
            <Descriptions bordered column={2}>
              <Descriptions.Item label="Name">
                {selectedStatus.name}
              </Descriptions.Item>
              <Descriptions.Item label="Type">
                <Tag>
                  {(selectedProfile.ha_type || 'generic').toUpperCase()}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Status">
                <Tag color={statusColor[selectedStatus.status]}>
                  {selectedStatus.status.toUpperCase()}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Active Node">
                {selectedStatus.active_node || 'N/A'}
              </Descriptions.Item>
              <Descriptions.Item label="VIP Active">
                {selectedStatus.vip_active ? (
                  <Tag color="green">Yes</Tag>
                ) : (
                  <Tag>No</Tag>
                )}
              </Descriptions.Item>
            </Descriptions>

            {selectedProfile.ha_type === 'nfs' && selectedProfile.nfs && (
              <Card title="NFS Configuration" size="small">
                <Descriptions bordered column={1}>
                  <Descriptions.Item label="Export Path">
                    {selectedProfile.nfs.export_path}
                  </Descriptions.Item>
                  <Descriptions.Item label="Allowed Networks">
                    {selectedProfile.nfs.allowed_networks.join(', ')}
                  </Descriptions.Item>
                  <Descriptions.Item label="Options">
                    {selectedProfile.nfs.options}
                  </Descriptions.Item>
                </Descriptions>
              </Card>
            )}

            {selectedProfile.ha_type === 'iscsi' && selectedProfile.iscsi && (
              <Card title="iSCSI Configuration" size="small">
                <Descriptions bordered column={1}>
                  <Descriptions.Item label="Target IQN">
                    {selectedProfile.iscsi.iqn}
                  </Descriptions.Item>
                  <Descriptions.Item label="Allowed Initiators">
                    {selectedProfile.iscsi.allowed_initiators.length > 0
                      ? selectedProfile.iscsi.allowed_initiators.join(', ')
                      : 'All'}
                  </Descriptions.Item>
                </Descriptions>
              </Card>
            )}

            {selectedProfile.ha_type === 'nvmeof' && selectedProfile.nvmeof && (
              <Card title="NVMe-oF Configuration" size="small">
                <Descriptions bordered column={1}>
                  <Descriptions.Item label="Target NQN">
                    {selectedProfile.nvmeof.nqn}
                  </Descriptions.Item>
                  <Descriptions.Item label="Fabric Type">
                    {selectedProfile.nvmeof.fabric_type.toUpperCase()}
                  </Descriptions.Item>
                  <Descriptions.Item label="Port">
                    {selectedProfile.nvmeof.trsvcid}
                  </Descriptions.Item>
                </Descriptions>
              </Card>
            )}

            {selectedStatus.drbd && (
              <Card title="DRBD Status" size="small">
                <Descriptions bordered column={2}>
                  <Descriptions.Item label="Resource">
                    {selectedStatus.drbd.resource}
                  </Descriptions.Item>
                  <Descriptions.Item label="Role">
                    <Tag color={roleColor[selectedStatus.drbd.role]}>
                      {selectedStatus.drbd.role}
                    </Tag>
                  </Descriptions.Item>
                  <Descriptions.Item label="Disk">
                    {selectedStatus.drbd.disk}
                  </Descriptions.Item>
                  <Descriptions.Item label="Open">
                    {selectedStatus.drbd.open ? 'Yes' : 'No'}
                  </Descriptions.Item>
                </Descriptions>
              </Card>
            )}

            <Card title="Services" size="small">
              <Table
                dataSource={selectedStatus.service_statuses}
                columns={[
                  { title: 'Service', dataIndex: 'name', key: 'name' },
                  {
                    title: 'Active',
                    dataIndex: 'active',
                    key: 'active',
                    render: (active: boolean) => (
                      <Tag color={active ? 'green' : 'default'}>
                        {active ? 'Running' : 'Stopped'}
                      </Tag>
                    ),
                  },
                  { title: 'State', dataIndex: 'state', key: 'state' },
                  {
                    title: 'Enabled',
                    dataIndex: 'enabled',
                    key: 'enabled',
                    render: (enabled: boolean) => (enabled ? 'Yes' : 'No'),
                  },
                ]}
                rowKey="name"
                pagination={false}
                size="small"
              />
            </Card>
          </div>
        )}
      </Modal>

      {/* Delete Confirmation Modal */}
      <Modal
        title={
          <span>
            <ExclamationCircleOutlined
              style={{ color: '#faad14', marginRight: 8 }}
            />
            Delete HA Profile
          </span>
        }
        open={deleteModalOpen}
        onCancel={() => setDeleteModalOpen(false)}
        onOk={handleDelete}
        okText="Delete"
        okButtonProps={{ danger: true, loading: deleting }}
        cancelButtonProps={{ disabled: deleting }}
      >
        {profileToDelete && (
          <div className="space-y-4">
            <p>
              Are you sure you want to delete the HA profile{' '}
              <strong>{profileToDelete.name}</strong>?
            </p>
            <div className="p-3 bg-gray-50 rounded">
              <Checkbox
                checked={deleteResource}
                onChange={(e) => setDeleteResource(e.target.checked)}
              >
                Also delete DRBD resource{' '}
                <strong>{profileToDelete.resource_name}</strong>
              </Checkbox>
              <p className="text-gray-500 text-sm mt-2 ml-6">
                This will remove the DRBD configuration files from all nodes.
                The underlying disk data will NOT be erased.
              </p>
            </div>
          </div>
        )}
      </Modal>

      {/* Add VIP Modal */}
      <Modal
        title={`Add VIP to ${selectedProfileForVip?.name || 'Profile'}`}
        open={vipModalOpen}
        onCancel={() => {
          setVipModalOpen(false);
          setSelectedProfileForVip(null);
        }}
        footer={null}
        destroyOnClose
      >
        <Form form={vipForm} layout="vertical" onFinish={handleAddVip}>
          <Form.Item
            name="address"
            label="IP Address"
            rules={[
              { required: true, message: 'Please enter IP address' },
              {
                pattern: /^(\d{1,3}\.){3}\d{1,3}$/,
                message: 'Invalid IP address format',
              },
            ]}
          >
            <Input placeholder="192.168.1.100" />
          </Form.Item>
          <Form.Item
            name="netmask"
            label="Netmask (CIDR)"
            rules={[{ required: true, message: 'Please enter netmask' }]}
          >
            <InputNumber min={1} max={32} className="w-full" />
          </Form.Item>
          <Form.Item
            name="interface"
            label="Network Interface"
            rules={[{ required: true, message: 'Please enter interface name' }]}
          >
            <Input placeholder="eth0" />
          </Form.Item>
          <Form.Item>
            <Button
              type="primary"
              htmlType="submit"
              loading={vipSubmitting}
              block
            >
              Add VIP
            </Button>
          </Form.Item>
        </Form>
      </Modal>
    </div>
  );
}

const roleColor: Record<string, string> = {
  Primary: 'green',
  Secondary: 'blue',
  Unknown: 'default',
};
