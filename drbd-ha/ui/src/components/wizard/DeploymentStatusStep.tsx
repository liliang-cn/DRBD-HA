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
              <Descriptions.Item label="All Services Active">
                {statusData.service_statuses && statusData.service_statuses.length > 0 ? (
                  statusData.service_statuses.every((s: any) => s.active) ? (
                    <Tag color="green">Yes</Tag>
                  ) : (
                    <Tag color="red">No</Tag>
                  )
                ) : (
                  <Tag color="default">Unknown</Tag>
                )}
              </Descriptions.Item>
              <Descriptions.Item label="DRBD Reactor">
                <Tag color={statusData.config?.reactor_running ? 'green' : 'red'}>
                  {statusData.config?.reactor_running ? 'Running' : 'Stopped'}
                </Tag>
              </Descriptions.Item>
            </Descriptions>
          </Card>

          {/* DRBD Status */}
          {statusData.drbd && (
            <Card title="DRBD Resource Status" size="small">
              <Descriptions bordered column={2} size="small">
                <Descriptions.Item label="Resource">
                  {statusData.drbd.resource}
                </Descriptions.Item>
                {statusData.drbd_device && (
                  <Descriptions.Item label="DRBD Device">
                    {statusData.drbd_device}
                  </Descriptions.Item>
                )}
                <Descriptions.Item label="Role">
                  <Tag color={roleColor[statusData.drbd.role] || 'default'}>
                    {statusData.drbd.role}
                  </Tag>
                </Descriptions.Item>
                <Descriptions.Item label="Disk State">
                  <Tag
                    color={
                      statusData.drbd.disk === 'UpToDate'
                        ? 'green'
                        : 'orange'
                    }
                  >
                    {statusData.drbd.disk}
                  </Tag>
                </Descriptions.Item>
                <Descriptions.Item label="Device Open">
                  {statusData.drbd.open ? (
                    <Tag color="green">Yes</Tag>
                  ) : (
                    <Tag color="default">No</Tag>
                  )}
                </Descriptions.Item>
                {statusData.drbd.peers && statusData.drbd.peers.length > 0 && (
                  <Descriptions.Item label="Connection State" span={2}>
                    {statusData.drbd.peers.map((peer: any, idx: number) => (
                      <div key={idx} className="mb-1">
                        <Tag color={
                          peer.connection === 'Connected' ? 'green' : 'orange'
                        }>
                          {peer.name}: {peer.connection || 'Unknown'}
                        </Tag>
                        {peer.replication && (
                          <Tag color="blue" className="ml-1">
                            {peer.replication}
                          </Tag>
                        )}
                      </div>
                    ))}
                  </Descriptions.Item>
                )}
                {statusData.drbd.peers && statusData.drbd.peers.some((p: any) => p.sync_percent !== undefined) && (
                  <Descriptions.Item label="Sync Progress" span={2}>
                    {statusData.drbd.peers.map((peer: any, idx: number) => (
                      peer.sync_percent !== undefined ? (
                        <div key={idx} className="mb-1">
                          <span className="mr-2">{peer.name}:</span>
                          <Progress
                            percent={Math.round(peer.sync_percent)}
                            status={peer.sync_percent >= 100 ? 'success' : 'active'}
                            style={{ display: 'inline-block', width: '200px' }}
                            size="small"
                          />
                        </div>
                      ) : null
                    ))}
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
                        {svc.state}
                        {svc.enabled !== undefined && (
                          <span className="ml-2">
                            ({svc.enabled ? 'enabled' : 'disabled'})
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-2">
                      {svc.active ? (
                        <CheckCircleOutlined className="text-green-500" />
                      ) : (
                        <ExclamationCircleOutlined className="text-red-500" />
                      )}
                      <Tag color={svc.active ? 'green' : 'red'}>
                        {svc.active ? 'Active' : 'Inactive'}
                      </Tag>
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          )}

          {/* Reactor Status */}
          <Card title="DRBD Reactor Status" size="small">
            <Descriptions bordered column={1} size="small">
              <Descriptions.Item label="Status">
                <Tag color={statusData.config?.reactor_running ? 'green' : 'red'}>
                  {statusData.config?.reactor_running ? 'Running' : 'Stopped'}
                </Tag>
              </Descriptions.Item>
              {statusData.mount_point && (
                <Descriptions.Item label="Mount Point">
                  {statusData.mount_point}
                </Descriptions.Item>
              )}
            </Descriptions>
          </Card>

          {/* Configured Nodes */}
          {statusData.configured_nodes && statusData.configured_nodes.length > 0 && (
            <Card title="Configured Nodes" size="small">
              <div className="space-y-2">
                {statusData.configured_nodes.map((node: any, idx: number) => (
                  <div key={idx} className="flex items-center justify-between p-2 bg-gray-50 rounded">
                    <div className="flex-1">
                      <div className="font-medium">{node.hostname}</div>
                      <div className="text-xs text-gray-500">{node.ip}</div>
                    </div>
                    {node.peer_role && (
                      <Tag color={roleColor[node.peer_role] || 'default'}>
                        {node.peer_role}
                      </Tag>
                    )}
                  </div>
                ))}
              </div>
            </Card>
          )}

          {/* Success Message */}
          {statusData.status === 'active' && statusData.service_statuses && statusData.service_statuses.length > 0 && statusData.service_statuses.every((s: any) => s.active) && (
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
