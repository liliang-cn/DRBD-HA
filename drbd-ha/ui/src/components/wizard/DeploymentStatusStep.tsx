import {
  CheckCircleOutlined,
  ExclamationCircleOutlined,
  FileTextOutlined,
  LoadingOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import { Button, Card, Descriptions, message, Progress, Result, Space, Tag } from 'antd';
import { useEffect, useState } from 'react';
import { haProfilesApi } from '@/api';

const statusColor: Record<string, string> = {
  active: 'green',
  standby: 'blue',
  stopped: 'default',
  error: 'red',
  unknown: 'default',
};

const roleColor: Record<string, string> = {
  Primary: 'green',
  Secondary: 'blue',
  Unknown: 'default',
};

interface DeploymentStatusStepProps {
  profileId: string | null;
  profileName: string | null;
  onDone?: () => void;
}

export function DeploymentStatusStep({
  profileId,
  profileName,
  onDone,
}: DeploymentStatusStepProps) {
  const [loading, setLoading] = useState(true);
  const [statusData, setStatusData] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);

  const fetchStatus = async () => {
    if (!profileId) {
      setError('No profile ID available');
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);

    try {
      const status = await haProfilesApi.getStatus(profileId);
      setStatusData(status);
    } catch (err) {
      const errMsg = (err as { message: string }).message;
      setError(errMsg);
      message.error(errMsg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchStatus();
  }, [profileId]);

  if (loading) {
    return (
      <Card title="Step 4: Deployment Status" className="max-w-4xl mx-auto">
        <div className="py-12 text-center">
          <LoadingOutlined className="text-4xl text-blue-500 mb-4" />
          <div className="text-lg">Checking deployment status...</div>
        </div>
      </Card>
    );
  }

  if (error) {
    return (
      <Card title="Step 4: Deployment Status" className="max-w-4xl mx-auto">
        <Result
          status="error"
          title="Failed to Check Deployment Status"
          subTitle={error}
          extra={
            <Space>
              <Button type="primary" icon={<ReloadOutlined />} onClick={fetchStatus}>
                Retry
              </Button>
              <Button onClick={onDone}>Go to Dashboard</Button>
            </Space>
          }
        />
      </Card>
    );
  }

  return (
    <Card
      title="Step 4: Deployment Status"
      className="max-w-4xl mx-auto"
      extra={
        <Button icon={<ReloadOutlined />} onClick={fetchStatus}>
          Refresh
        </Button>
      }
    >
      {statusData && (
        <div className="space-y-4">
          {/* Status Overview */}
          <Card title={<><FileTextOutlined /> Status Overview</>} size="small">
            <Descriptions bordered column={2} size="small">
              <Descriptions.Item label="Profile Status">
                <Tag color={statusColor[statusData.status] || 'default'}>
                  {statusData.status?.toUpperCase()}
                </Tag>
              </Descriptions.Item>
              <Descriptions.Item label="Active Node">
                {statusData.active_node || '-'}
              </Descriptions.Item>
              <Descriptions.Item label="Local Node Active">
                {statusData.is_local_active ? (
                  <Tag color="green">Yes</Tag>
                ) : (
                  <Tag color="default">No</Tag>
                )}
              </Descriptions.Item>
              <Descriptions.Item label="All Services Running">
                {statusData.all_services_running ? (
                  <Tag color="green">Yes</Tag>
                ) : (
                  <Tag color="red">No</Tag>
                )}
              </Descriptions.Item>
            </Descriptions>
          </Card>

          {/* DRBD Status */}
          {statusData.drbd_status && (
            <Card title="DRBD Resource Status" size="small">
              <Descriptions bordered column={2} size="small">
                <Descriptions.Item label="Resource">
                  {statusData.drbd_status.resource_name}
                </Descriptions.Item>
                <Descriptions.Item label="Role">
                  <Tag color={roleColor[statusData.drbd_status.local_role] || 'default'}>
                    {statusData.drbd_status.local_role}
                  </Tag>
                </Descriptions.Item>
                <Descriptions.Item label="Disk State">
                  <Tag
                    color={
                      statusData.drbd_status.local_disk_state === 'UpToDate'
                        ? 'green'
                        : 'orange'
                    }
                  >
                    {statusData.drbd_status.local_disk_state}
                  </Tag>
                </Descriptions.Item>
                <Descriptions.Item label="Connection State">
                  <Tag
                    color={
                      statusData.drbd_status.connection_state === 'Connected'
                        ? 'green'
                        : 'orange'
                    }
                  >
                    {statusData.drbd_status.connection_state}
                  </Tag>
                </Descriptions.Item>
                {statusData.drbd_status.sync_progress_percent !== undefined && (
                  <Descriptions.Item label="Sync Progress" span={2}>
                    <Progress
                      percent={Math.round(
                        statusData.drbd_status.sync_progress_percent * 100
                      )}
                      status={statusData.drbd_status.sync_progress_percent >= 1 ? 'success' : 'active'}
                      size="small"
                    />
                  </Descriptions.Item>
                )}
              </Descriptions>
            </Card>
          )}

          {/* Service Status */}
          {statusData.service_statuses && statusData.service_statuses.length > 0 && (
            <Card title="Service Status" size="small">
              <div className="space-y-2">
                {statusData.service_statuses.map((svc: any, idx: number) => (
                  <div key={idx} className="flex items-center justify-between p-2 bg-gray-50 rounded">
                    <div className="flex-1">
                      <div className="font-medium">{svc.name}</div>
                      <div className="text-xs text-gray-500">
                        {svc.active_state} / {svc.sub_state}
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      {svc.running ? (
                        <CheckCircleOutlined className="text-green-500" />
                      ) : (
                        <ExclamationCircleOutlined className="text-red-500" />
                      )}
                      <Tag color={svc.running ? 'green' : 'red'}>
                        {svc.running ? 'Running' : 'Stopped'}
                      </Tag>
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          )}

          {/* Reactor Status */}
          {statusData.reactor_status && (
            <Card title="DRBD Reactor Status" size="small">
              <Descriptions bordered column={1} size="small">
                <Descriptions.Item label="Status">
                  <Tag color={statusData.reactor_status.running ? 'green' : 'red'}>
                    {statusData.reactor_status.running ? 'Running' : 'Stopped'}
                  </Tag>
                </Descriptions.Item>
                {statusData.reactor_status.promoter_status && (
                  <Descriptions.Item label="Promoter">
                    {statusData.reactor_status.promoter_status}
                  </Descriptions.Item>
                )}
              </Descriptions>
            </Card>
          )}

          {/* Mount Status */}
          {statusData.mount_status && (
            <Card title="Mount Status" size="small">
              <Descriptions bordered column={2} size="small">
                <Descriptions.Item label="Mounted">
                  {statusData.mount_status.mounted ? (
                    <Tag color="green">Yes</Tag>
                  ) : (
                    <Tag color="red">No</Tag>
                  )}
                </Descriptions.Item>
                <Descriptions.Item label="Mount Point">
                  {statusData.mount_status.mount_point || '-'}
                </Descriptions.Item>
                {statusData.mount_status.fs_type && (
                  <Descriptions.Item label="File System" span={2}>
                    {statusData.mount_status.fs_type}
                  </Descriptions.Item>
                )}
              </Descriptions>
            </Card>
          )}

          {/* Success Message */}
          {statusData.status === 'active' && statusData.all_services_running && (
            <Result
              status="success"
              title="Deployment Successful!"
              subTitle={`HA profile '${profileName}' is active and all services are running on ${statusData.active_node || 'the local node'}.`}
              extra={
                <Button type="primary" onClick={onDone}>
                  Go to Dashboard
                </Button>
              }
            />
          )}

          {/* Warning Message */}
          {statusData.status !== 'active' && (
            <Result
              status="warning"
              title="Deployment Completed"
              subTitle={`HA profile '${profileName}' has been created, but the status is '${statusData.status}'. Check the details above for more information.`}
              extra={
                <Space>
                  <Button type="primary" onClick={fetchStatus}>
                    Refresh Status
                  </Button>
                  <Button onClick={onDone}>Go to Dashboard</Button>
                </Space>
              }
            />
          )}
        </div>
      )}
    </Card>
  );
}
