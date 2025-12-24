import {
  CheckCircleOutlined,
  DeleteOutlined,
  ExclamationCircleOutlined,
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
  Popconfirm,
  Progress,
  Space,
  Table,
  Tag,
  Typography,
} from 'antd';
import { useEffect, useRef, useState } from 'react';
import { haProfilesApi } from '@/api';
import { ImportProfilesModal } from '@/components/ha/ImportProfilesModal';
import { useHaProfilesStore } from '@/stores/ha-profiles';
import { useNotificationsStore } from '@/stores/notifications';
import { useResourcesStore } from '@/stores/resources';
import type { HaProfile, HaProfileStatus } from '@/types';

const statusColor: Record<string, string> = {
  active: 'green',
  standby: 'blue',
  stopped: 'default',
  error: 'red',
  unknown: 'default',
};

export function HaProfiles() {
  const { profiles, loading, fetch } = useHaProfilesStore();
  const { fetch: fetchResources } = useResourcesStore();
  const [expandedProfileId, setExpandedProfileId] = useState<string | null>(
    null,
  );
  const [profileStatuses, setProfileStatuses] = useState<
    Record<string, HaProfileStatus>
  >({});
  const [statusLoading, setStatusLoading] = useState<Record<string, boolean>>(
    {},
  );
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [profileToDelete, setProfileToDelete] = useState<HaProfile | null>(
    null,
  );
  const [deleteResource, setDeleteResource] = useState(true);
  const [deleting, setDeleting] = useState(false);
  const [importModalOpen, setImportModalOpen] = useState(false);

  // Deletion Progress State
  const [progressModalOpen, setProgressModalOpen] = useState(false);
  const [deletionLogs, setDeletionLogs] = useState<string[]>([]);
  const [deletingProfileName, setDeletingProfileName] = useState<string | null>(
    null,
  );
  const [deletionProgressSteps, setDeletionProgressSteps] = useState<
    Array<{ message: string; done: boolean }>
  >([]);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const progressEvents = useNotificationsStore((s) => s.progress);
  const processedMessageIds = useRef<Set<string>>(new Set());

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

    const relevantProgress = (progressEvents || []).filter((p) => {
      // Show all progress events for the target resource
      if (p.resource === deletingProfileName) {
        return true; // Accept any operation for the target resource
      }

      // Also show general system progress events that might be related to deletion
      if (
        !p.resource &&
        (p.operation === 'cleanup' ||
          p.operation === 'system_maintenance' ||
          p.operation === 'cluster_sync' ||
          p.operation === 'config_reload')
      ) {
        return true;
      }

      return false;
    });

    if (relevantProgress.length > 0) {
      // Sort by operation_id to maintain order
      const sortedProgress = relevantProgress.sort((a, b) =>
        a.operation_id.localeCompare(b.operation_id),
      );

      // Update progress steps display
      const newSteps = sortedProgress
        .map((p) => ({
          message: p.message,
          done: p.completed,
        }))
        .filter((s) => s.message);

      if (newSteps.length > 0) {
        // Update deletion progress steps in the UI
        setDeletionProgressSteps(newSteps);
      }

      // Process each progress event
      sortedProgress.forEach((progress) => {
        const messageId = `${progress.operation_id}_${progress.progress}_${progress.message}`;

        if (progress.message && !processedMessageIds.current.has(messageId)) {
          setDeletionLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] ${progress.message}`,
          ]);
          processedMessageIds.current.add(messageId);
        }

        // Check for completion and errors
        if (progress.completed && progress.success === false) {
          setDeletionLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] ERROR: ${progress.message}`,
          ]);
          setDeleting(false); // Stop loading but keep modal open
        } else if (progress.completed && progress.success === true) {
          setDeletionLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] Deletion completed successfully.`,
          ]);
          setDeleting(false);
          // Keep modal open for a moment to show success
          setTimeout(() => {
            setProgressModalOpen(false);
            setDeletingProfileName(null);
            setProfileToDelete(null);
            fetch();
            fetchResources();
          }, 2000);
        }
      });
    }
  }, [progressEvents, deletingProfileName, progressModalOpen]);

  const openDeleteModal = (profile: HaProfile) => {
    setProfileToDelete(profile);
    setDeleteResource(true);
    setDeleteModalOpen(true);
  };

  const handleDelete = async () => {
    if (!profileToDelete) return;

    // Switch to progress modal and reset state
    setDeletingProfileName(profileToDelete.name);
    setDeletionLogs([]);
    setDeletionProgressSteps([]);
    processedMessageIds.current.clear();
    setDeleteModalOpen(false);
    setProgressModalOpen(true);
    setDeleting(true);

    // Add initial log
    setDeletionLogs([
      `[${new Date().toLocaleTimeString()}] Requesting deletion of ${profileToDelete.name}...`,
    ]);

    try {
      // Make the API call - progress will be handled by SSE
      await haProfilesApi.delete(profileToDelete.id, deleteResource);

      // API call successful - SSE will handle the rest
      setDeletionLogs((prev) => [
        ...prev,
        `[${new Date().toLocaleTimeString()}] Delete request sent successfully. Waiting for completion...`,
      ]);
    } catch (err) {
      const errMsg = (err as { message: string }).message;
      setDeletionLogs((prev) => [
        ...prev,
        `[${new Date().toLocaleTimeString()}] ERROR: Failed to send delete request: ${errMsg}`,
      ]);
      message.error(errMsg);
      // Stop loading but keep modal open so user can see the error
      setDeleting(false);
    }
  };

  const handleCloseProgressModal = () => {
    setProgressModalOpen(false);
    setDeletingProfileName(null);
    setDeletionProgressSteps([]);
    setProfileToDelete(null);
    setDeleting(false);
    processedMessageIds.current.clear();
    fetch();
    fetchResources();
  };

  const handleRowExpand = async (expanded: boolean, record: HaProfile) => {
    if (expanded) {
      setExpandedProfileId(record.id);
      // Fetch status if not already loaded
      if (!profileStatuses[record.id]) {
        setStatusLoading((prev) => ({ ...prev, [record.id]: true }));
        try {
          const status = await haProfilesApi.getStatus(record.id);
          setProfileStatuses((prev) => ({ ...prev, [record.id]: status }));
        } catch (err) {
          message.error((err as { message: string }).message);
        } finally {
          setStatusLoading((prev) => ({ ...prev, [record.id]: false }));
        }
      }
    } else {
      setExpandedProfileId(null);
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
              <Popconfirm
                title="Evict Profile"
                description={`Are you sure you want to evict the HA profile from ${node}? This will trigger failover to a standby node.`}
                onConfirm={() => handleEvict(record.id)}
                okText="Evict"
                cancelText="Cancel"
                okButtonProps={{ danger: true }}
                icon={<ExclamationCircleOutlined style={{ color: 'red' }} />}
              >
                <Button size="small" danger title="Evict">
                  Evict
                </Button>
              </Popconfirm>
            )}
          </Space>
        );
      },
    },
    {
      title: 'Actions',
      key: 'actions',
      render: (_: unknown, record: HaProfile) => {
        return (
          <Space wrap>
            <Button
              size="small"
              type="text"
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

  // Render expanded row content
  const expandedRowRender = (record: HaProfile) => {
    const status = profileStatuses[record.id];
    const isLoading = statusLoading[record.id];

    if (isLoading) {
      return (
        <div className="flex justify-center items-center p-8">
          <LoadingOutlined className="text-2xl text-blue-500" />
        </div>
      );
    }

    return (
      <div className="p-4 space-y-4">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* Basic Info */}
          <Card title="Profile Information" size="small">
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label="ID">{record.id}</Descriptions.Item>
              <Descriptions.Item label="Name">{record.name}</Descriptions.Item>
              <Descriptions.Item label="Type">
                <Tag>{(record.ha_type || 'generic').toUpperCase()}</Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Resource Name">
                {record.resource_name}
              </Descriptions.Item>
              <Descriptions.Item label="Mount Point">
                {record.mount_point}
              </Descriptions.Item>
              <Descriptions.Item label="File System">
                {record.fs_type}
              </Descriptions.Item>
              <Descriptions.Item label="DRBD Device">
                {record.generated_units.drbd_device}
              </Descriptions.Item>
            </Descriptions>
          </Card>

          {/* Promoter Configuration */}
          <Card title="Promoter Configuration" size="small">
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label="Services">
                <Space wrap>
                  {record.promoter.services.map((s) => (
                    <Tag key={s}>{s}</Tag>
                  ))}
                </Space>
              </Descriptions.Item>
              <Descriptions.Item label="Stop on Demote">
                {record.promoter.stop_on_demote ? 'Yes' : 'No'}
              </Descriptions.Item>
              <Descriptions.Item label="On Demote Failure">
                {record.promoter.on_demote_failure}
              </Descriptions.Item>
              {record.promoter.dependencies_as && (
                <Descriptions.Item label="Dependencies AS">
                  {record.promoter.dependencies_as}
                </Descriptions.Item>
              )}
              {record.promoter.target_as && (
                <Descriptions.Item label="Target AS">
                  {record.promoter.target_as}
                </Descriptions.Item>
              )}
              {record.promoter.preferred_nodes &&
                record.promoter.preferred_nodes.length > 0 && (
                  <Descriptions.Item label="Preferred Nodes">
                    <Space wrap>
                      {record.promoter.preferred_nodes.map((n) => (
                        <Tag key={n}>{n}</Tag>
                      ))}
                    </Space>
                  </Descriptions.Item>
                )}
            </Descriptions>
          </Card>

          {/* OCF Agents */}
          {record.ocf_agents && record.ocf_agents.length > 0 && (
            <Card title="OCF Agents" size="small">
              <Table
                dataSource={record.ocf_agents}
                columns={[
                  { title: 'Name', dataIndex: 'name', key: 'name' },
                  {
                    title: 'Instance',
                    dataIndex: 'instance_name',
                    key: 'instance_name',
                  },
                  {
                    title: 'Parameters',
                    dataIndex: 'params',
                    key: 'params',
                    render: (params: Record<string, string>) => (
                      <Space direction="vertical" size="small">
                        {Object.entries(params).map(([k, v]) => (
                          <div key={k}>
                            <Tag color="blue">{k}</Tag>: {v}
                          </div>
                        ))}
                      </Space>
                    ),
                  },
                ]}
                rowKey="instance_name"
                pagination={false}
                size="small"
              />
            </Card>
          )}
        </div>

        {/* DRBD Status */}
        {status && status.drbd && (
          <Card title="DRBD Status" size="small">
            <Descriptions bordered column={2} size="small">
              <Descriptions.Item label="Resource">
                {status.drbd.resource}
              </Descriptions.Item>
              <Descriptions.Item label="Role">
                <Tag color={roleColor[status.drbd.role]}>
                  {status.drbd.role}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Disk State">
                {status.drbd.disk}
              </Descriptions.Item>
              <Descriptions.Item label="Device Open">
                {status.drbd.open ? 'Yes' : 'No'}
              </Descriptions.Item>
            </Descriptions>

            {status.drbd.peers && status.drbd.peers.length > 0 && (
              <div className="mt-4">
                <h4 className="font-semibold mb-2">Peers</h4>
                <div className="space-y-2">
                  {status.drbd.peers.map((peer) => (
                    <Descriptions
                      key={peer.name}
                      title={peer.name}
                      bordered
                      column={1}
                      size="small"
                    >
                      <Descriptions.Item label="Role">
                        <Tag color={roleColor[peer.role]}>{peer.role}</Tag>
                      </Descriptions.Item>
                      <Descriptions.Item label="Disk State">
                        {peer.peer_disk}
                      </Descriptions.Item>
                      {peer.connection && (
                        <Descriptions.Item label="Connection">
                          {peer.connection}
                        </Descriptions.Item>
                      )}
                      {peer.replication && (
                        <Descriptions.Item label="Replication">
                          {peer.replication}
                        </Descriptions.Item>
                      )}
                    </Descriptions>
                  ))}
                </div>
              </div>
            )}
          </Card>
        )}

        {/* Services Status */}
        {status && status.service_statuses && (
          <Card title="Services Status" size="small">
            <Table
              dataSource={status.service_statuses}
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
        )}

        {/* System Configuration */}
        {status && status.config && (
          <Card title="System Configuration" size="small">
            <Descriptions bordered column={2} size="small">
              <Descriptions.Item label="Promoter Config Exists">
                {status.config.promoter_config_exists ? (
                  <Tag color="green">Yes</Tag>
                ) : (
                  <Tag color="red">No</Tag>
                )}
              </Descriptions.Item>
              <Descriptions.Item label="Promoter Config Path">
                {status.config.promoter_config_path}
              </Descriptions.Item>
              <Descriptions.Item label="Reactor Running">
                {status.config.reactor_running ? (
                  <Tag color="green">Yes</Tag>
                ) : (
                  <Tag color="red">No</Tag>
                )}
              </Descriptions.Item>
            </Descriptions>
          </Card>
        )}
      </div>
    );
  };

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
            onClick={() => (window.location.href = '/service-ha/create')}
          >
            Create Service HA
          </Button>
          {/* <Button
            icon={<PlusOutlined />}
            onClick={() => navigate('/storage-sharing/create')}
          >
            Create Storage HA
          </Button> */}
        </Space>
      </div>

      <Table
        dataSource={profiles}
        columns={columns}
        rowKey="id"
        loading={loading}
        pagination={false}
        expandable={{
          expandedRowRender,
          onExpand: handleRowExpand,
          expandedRowKeys: expandedProfileId ? [expandedProfileId] : [],
        }}
      />

      <ImportProfilesModal
        open={importModalOpen}
        onCancel={() => setImportModalOpen(false)}
        onSuccess={() => {
          fetch();
          fetchResources();
        }}
      />

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
        <div className="space-y-4">
          {/* Progress Steps */}
          {deletionProgressSteps.length > 0 && (
            <div className="space-y-2">
              <div className="text-sm font-medium text-gray-700 mb-2">
                Deletion Progress:
              </div>
              {deletionProgressSteps.map((step, idx) => (
                <div key={idx} className="flex items-start gap-2 text-sm">
                  {step.done ? (
                    <CheckCircleOutlined className="text-green-500 mt-1 shrink-0" />
                  ) : (
                    <LoadingOutlined className="text-blue-500 mt-1 shrink-0" />
                  )}
                  <span
                    className={
                      step.done ? 'text-gray-700' : 'text-blue-600 font-medium'
                    }
                  >
                    {step.message}
                  </span>
                </div>
              ))}
            </div>
          )}

          {/* Logs */}
          <div className="h-[200px] overflow-y-auto bg-gray-50 p-4 rounded font-mono text-xs border border-gray-200">
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
        </div>
      </Modal>
    </div>
  );
}

const roleColor: Record<string, string> = {
  Primary: 'green',
  Secondary: 'blue',
  Unknown: 'default',
};
