import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  DeleteOutlined,
  ExclamationCircleOutlined,
  EyeOutlined,
  LoadingOutlined,
  PlusOutlined,
} from '@ant-design/icons';
import {
  Button,
  Card,
  Checkbox,
  Descriptions,
  Form,
  Input,
  InputNumber,
  Modal,
  message,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { haProfilesApi } from '@/api';
import { ImportProfilesModal } from '@/components/ha/ImportProfilesModal';
import { useHaProfilesStore } from '@/stores/ha-profiles';
import { useNotificationsStore } from '@/stores/notifications';
import { useResourcesStore } from '@/stores/resources';
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
  const { profiles, loading, fetch } = useHaProfilesStore();
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

  // Deletion Progress State
  const [progressModalOpen, setProgressModalOpen] = useState(false);
  const [deletionLogs, setDeletionLogs] = useState<string[]>([]);
  const [deletingProfileName, setDeletingProfileName] = useState<string | null>(
    null,
  );
  const logsEndRef = useRef<HTMLDivElement>(null);
  const progressEvents = useNotificationsStore((s) => s.progress);
  const lastLogCountRef = useRef(0);

  useEffect(() => {
    fetch();
    fetchResources();
  }, [fetch, fetchResources]);

  // Scroll logs to bottom
  useEffect(() => {
    if (progressModalOpen) {
      logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [deletionLogs, progressModalOpen]);

  // Listen for deletion progress
  useEffect(() => {
    if (!deletingProfileName || !progressModalOpen) return;

    const relevantProgress = progressEvents.filter(
      (p) =>
        p.resource === deletingProfileName &&
        p.operation === 'delete_ha_profile',
    );

    if (relevantProgress.length > lastLogCountRef.current) {
      const newEvents = relevantProgress.slice(lastLogCountRef.current);
      newEvents.forEach((e) => {
        if (e.message) {
          setDeletionLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] ${e.message}`,
          ]);
        }
      });
      lastLogCountRef.current = relevantProgress.length;
    }
  }, [progressEvents, deletingProfileName, progressModalOpen]);

  const openDeleteModal = (profile: HaProfile) => {
    setProfileToDelete(profile);
    setDeleteResource(true);
    setDeleteModalOpen(true);
  };

  const handleDelete = async () => {
    if (!profileToDelete) return;

    // Switch to progress modal
    setDeletingProfileName(profileToDelete.name);
    setDeletionLogs([]);
    lastLogCountRef.current = 0;
    setDeleteModalOpen(false);
    setProgressModalOpen(true);
    setDeleting(true);

    try {
      setDeletionLogs([
        `[${new Date().toLocaleTimeString()}] Requesting deletion...`,
      ]);
      await haProfilesApi.delete(profileToDelete.id, deleteResource);

      setDeletionLogs((prev) => [
        ...prev,
        `[${new Date().toLocaleTimeString()}] Deletion completed successfully.`,
      ]);
      message.success(
        deleteResource
          ? 'HA Profile and DRBD resource deleted'
          : 'HA Profile deleted',
      );

      // Keep modal open for a moment to show success
      setTimeout(() => {
        setProgressModalOpen(false);
        setDeletingProfileName(null);
        setProfileToDelete(null);
        fetch();
        fetchResources();
      }, 1500);
    } catch (err) {
      const errMsg = (err as { message: string }).message;
      setDeletionLogs((prev) => [
        ...prev,
        `[${new Date().toLocaleTimeString()}] ERROR: ${errMsg}`,
      ]);
      message.error(errMsg);
      // Keep modal open on error so user can see logs
      setDeleting(false); // Stop loading spinner but keep modal
    } finally {
      // If success, deleting is set to false in timeout
      // If error, set to false immediately
      if (!deletingProfileName) setDeleting(false);
    }
  };

  const handleCloseProgressModal = () => {
    setProgressModalOpen(false);
    setDeletingProfileName(null);
    setProfileToDelete(null);
    setDeleting(false);
    fetch();
    fetchResources();
  };

  const handleViewStatus = async (profile: HaProfile) => {
    navigate(`/ha-profiles/${profile.id}`);
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

  const _handleReloadReactor = async () => {
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

  const columns = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    {
      title: 'Type',
      dataIndex: 'ha_type',
      key: 'ha_type',
      render: (t: string) => <Tag>{(t || 'generic').toUpperCase()}</Tag>,
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
        return (
          <Tag color={statusColor[status] || 'default'}>
            {status.toUpperCase()}
          </Tag>
        );
      },
    },
    {
      title: 'Active Node',
      dataIndex: 'active_node',
      key: 'active_node',
      render: (node: string | undefined, record: HaProfile) => {
        const isActive = record.status === 'active';
        return (
          <Space>
            {node || '-'}
            {isActive && node && (
              <Button
                size="small"
                danger
                onClick={() => handleEvict(record.id)}
                title="Evict"
              >
                Evict
              </Button>
            )}
          </Space>
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
      render: (_: unknown, record: HaProfile) => {
        const isActive = record.status === 'active';
        const hasVip = !!record.vip;

        return (
          <Space wrap>
            <Button
              size="small"
              type="text" // Use text type for icon-only buttons
              icon={<EyeOutlined />}
              onClick={() => handleViewStatus(record)}
              title="View Status"
            />
            {!hasVip && (
              <Button
                size="small"
                onClick={() => openAddVipModal(record)}
                title="Add VIP"
              >
                Add VIP
              </Button>
            )}

            <Button
              size="small"
              type="text" // Use text type for icon-only buttons
              danger
              icon={<DeleteOutlined />}
              onClick={() => openDeleteModal(record)}
              title="Delete"
            />
          </Space>
        );
      },
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h2 className="text-2xl font-semibold">HA Profiles</h2>
        <Space>
          {/* <Button onClick={handleReloadReactor}>Reload drbd-reactor</Button>
          <Button
            icon={<SearchOutlined />}
            onClick={() => setImportModalOpen(true)}
          >
            Discover / Import
          </Button> */}
          <Button
            type="primary"
            icon={<PlusOutlined />}
            onClick={() => navigate('/service-ha/create')}
          >
            Create Service HA
          </Button>
          <Button
            icon={<PlusOutlined />}
            onClick={() => navigate('/storage-sharing/create')}
          >
            Create Storage HA
          </Button>
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

      {/* Deletion Progress Modal */}
      <Modal
        title={
          <span>
            <LoadingOutlined style={{ marginRight: 8 }} />
            Deleting Profile {deletingProfileName}
          </span>
        }
        open={progressModalOpen}
        onCancel={handleCloseProgressModal}
        footer={[
          <Button
            key="close"
            onClick={handleCloseProgressModal}
            disabled={deleting}
          >
            Close
          </Button>,
        ]}
        width={600}
        closable={!deleting}
        maskClosable={!deleting}
      >
        <div className="h-[300px] overflow-y-auto bg-gray-50 p-4 rounded font-mono text-xs border border-gray-200">
          {deletionLogs.length === 0 ? (
            <div className="text-gray-400 text-center mt-20">
              Waiting for logs...
            </div>
          ) : (
            deletionLogs.map((log, i) => (
              <div key={i} className="mb-1 text-gray-700">
                {log}
              </div>
            ))
          )}
          <div ref={logsEndRef} />
        </div>
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
