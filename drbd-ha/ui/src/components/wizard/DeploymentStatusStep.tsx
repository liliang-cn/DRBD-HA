import {
  CheckCircleOutlined,
  ExclamationCircleOutlined,
  FileTextOutlined,
  LoadingOutlined,
  ReloadOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons';
import { Button, Card, Descriptions, message, Progress, Result, Space, Tag, Typography } from 'antd';
import { useEffect, useState } from 'react';
import { haProfilesApi } from '@/api';
import { useThemeStore } from '@/stores/theme';
import { ACCENT_COLORS } from '@/theme/colors';

const { Title, Text } = Typography;

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
  const { theme: currentTheme } = useThemeStore();

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
      <div className="h-full flex flex-col">
        <Title level={3} className="!mb-6">
          <span
            className="bg-clip-text text-transparent"
            style={{
              backgroundImage: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
            }}
          >
            Step 5: Deployment Results
          </span>
        </Title>
        <div className="flex-1 flex flex-col items-center justify-center">
          <div
            className="w-20 h-20 rounded-2xl flex items-center justify-center mb-6"
            style={{
              background: `linear-gradient(135deg, ${ACCENT_COLORS.sky}20, ${ACCENT_COLORS.blue}20)`,
            }}
          >
            <LoadingOutlined
              className="text-4xl"
              style={{ color: ACCENT_COLORS.sky }}
            />
          </div>
          <Text className="text-lg">Checking deployment status...</Text>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-full flex flex-col">
        <Title level={3} className="!mb-6">
          <span
            className="bg-clip-text text-transparent"
            style={{
              backgroundImage: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
            }}
          >
            Step 5: Deployment Results
          </span>
        </Title>
        <div className="flex-1 flex items-center justify-center">
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
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <Title level={3} className="!mb-0">
          <span
            className="bg-clip-text text-transparent"
            style={{
              backgroundImage: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
            }}
          >
            Step 5: Deployment Results
          </span>
        </Title>
        <Button icon={<ReloadOutlined />} onClick={fetchStatus}>
          Refresh
        </Button>
      </div>

      {statusData && (
        <div className="flex-1 space-y-4 overflow-y-auto">
          {/* Success/Warning Message - Moved to top */}
          {statusData.status === 'active' && statusData.service_statuses && statusData.service_statuses.length > 0 && statusData.service_statuses.every((s: any) => s.active) ? (
            <Card
              className="shadow-sm border-l-4"
              style={{
                borderLeftColor: ACCENT_COLORS.mint,
                borderColor: currentTheme === 'dark' ? '#334155' : '#e2e8f0',
              }}
            >
              <div className="flex items-start gap-4">
                <div
                  className="w-16 h-16 rounded-2xl flex items-center justify-center shrink-0"
                  style={{
                    background: `linear-gradient(135deg, ${ACCENT_COLORS.mint}30, ${ACCENT_COLORS.cyan}30)`,
                  }}
                >
                  <CheckCircleOutlined
                    className="text-3xl"
                    style={{ color: ACCENT_COLORS.mint }}
                  />
                </div>
                <div className="flex-1">
                  <Title level={3} className="!mb-2">
                    Deployment Successful!
                  </Title>
                  <Text
                    className="text-base"
                    style={{ color: currentTheme === 'dark' ? '#94a3b8' : '#64748b' }}
                  >
                    HA profile '{profileName}' is active and all services are running on {statusData.active_node || 'the local node'}.
                  </Text>
                  <div className="mt-4">
                    <Button type="primary" size="large" onClick={onDone}>
                      Go to Dashboard
                    </Button>
                  </div>
                </div>
              </div>
            </Card>
          ) : statusData.status !== 'active' ? (
            <Card
              className="shadow-sm border-l-4"
              style={{
                borderLeftColor: ACCENT_COLORS.gold,
                borderColor: currentTheme === 'dark' ? '#334155' : '#e2e8f0',
              }}
            >
              <div className="flex items-start gap-4">
                <div
                  className="w-16 h-16 rounded-2xl flex items-center justify-center shrink-0"
                  style={{
                    background: `linear-gradient(135deg, ${ACCENT_COLORS.gold}30, ${ACCENT_COLORS.orange}30)`,
                  }}
                >
                  <ExclamationCircleOutlined
                    className="text-3xl"
                    style={{ color: ACCENT_COLORS.gold }}
                  />
                </div>
                <div className="flex-1">
                  <Title level={3} className="!mb-2">
                    Deployment Completed
                  </Title>
                  <Text
                    className="text-base"
                    style={{ color: currentTheme === 'dark' ? '#94a3b8' : '#64748b' }}
                  >
                    HA profile '{profileName}' has been created, but the status is '{statusData.status}'. Check the details below for more information.
                  </Text>
                  <div className="mt-4">
                    <Space>
                      <Button type="primary" onClick={fetchStatus}>
                        Refresh Status
                      </Button>
                      <Button onClick={onDone}>Go to Dashboard</Button>
                    </Space>
                  </div>
                </div>
              </div>
            </Card>
          ) : null}

          {/* Status Overview */}
          <Card
            className="shadow-sm"
            style={{
              borderColor: currentTheme === 'dark' ? '#334155' : '#e2e8f0',
            }}
          >
            <div className="flex items-center gap-3 mb-4">
              <div
                className="w-10 h-10 rounded-xl flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
                }}
              >
                <FileTextOutlined className="text-white text-lg" />
              </div>
              <Title level={4} className="!mb-0">
                Status Overview
              </Title>
            </div>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div
                className={`p-4 rounded-xl ${
                  currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                }`}
              >
                <div
                  className={`text-sm mb-2 ${
                    currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                  }`}
                >
                  Profile Status
                </div>
                <Tag
                  color={statusColor[statusData.status] || 'default'}
                  className="text-base px-3 py-1"
                >
                  {statusData.status?.toUpperCase()}
                </Tag>
              </div>
              <div
                className={`p-4 rounded-xl ${
                  currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                }`}
              >
                <div
                  className={`text-sm mb-2 ${
                    currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                  }`}
                >
                  Active Node
                </div>
                <div className="text-base font-semibold">
                  {statusData.active_node || '-'}
                </div>
              </div>
              <div
                className={`p-4 rounded-xl ${
                  currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                }`}
              >
                <div
                  className={`text-sm mb-2 ${
                    currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                  }`}
                >
                  All Services Active
                </div>
                {statusData.service_statuses && statusData.service_statuses.length > 0 ? (
                  statusData.service_statuses.every((s: any) => s.active) ? (
                    <Tag color="green" className="text-base px-3 py-1">
                      Yes
                    </Tag>
                  ) : (
                    <Tag color="red" className="text-base px-3 py-1">
                      No
                    </Tag>
                  )
                ) : (
                  <Tag color="default" className="text-base px-3 py-1">
                    Unknown
                  </Tag>
                )}
              </div>
              <div
                className={`p-4 rounded-xl ${
                  currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                }`}
              >
                <div
                  className={`text-sm mb-2 ${
                    currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                  }`}
                >
                  DRBD Reactor
                </div>
                <Tag
                  color={statusData.config?.reactor_running ? 'green' : 'red'}
                  className="text-base px-3 py-1"
                >
                  {statusData.config?.reactor_running ? 'Running' : 'Stopped'}
                </Tag>
              </div>
            </div>
          </Card>

          {/* DRBD Status */}
          {statusData.drbd && (
            <Card
              title={
                <div className="flex items-center gap-3">
                  <div
                    className="w-10 h-10 rounded-xl flex items-center justify-center"
                    style={{
                      background: `linear-gradient(135deg, ${ACCENT_COLORS.sky}30, ${ACCENT_COLORS.blue}30)`,
                    }}
                  >
                    <ThunderboltOutlined
                      className="text-lg"
                      style={{ color: ACCENT_COLORS.sky }}
                    />
                  </div>
                  <span>DRBD Resource Status</span>
                </div>
              }
              className="shadow-sm"
              style={{
                borderColor: currentTheme === 'dark' ? '#334155' : '#e2e8f0',
              }}
            >
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div
                  className={`p-3 rounded-lg ${
                    currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                  }`}
                >
                  <div
                    className={`text-xs mb-1 ${
                      currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                    }`}
                  >
                    Resource
                  </div>
                  <div className="font-medium">{statusData.drbd.resource}</div>
                </div>
                {statusData.drbd_device && (
                  <div
                    className={`p-3 rounded-lg ${
                      currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                    }`}
                  >
                    <div
                      className={`text-xs mb-1 ${
                        currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                      }`}
                    >
                      DRBD Device
                    </div>
                    <div className="font-medium">{statusData.drbd_device}</div>
                  </div>
                )}
                <div
                  className={`p-3 rounded-lg ${
                    currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                  }`}
                >
                  <div
                    className={`text-xs mb-1 ${
                      currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                    }`}
                  >
                    Role
                  </div>
                  <Tag color={roleColor[statusData.drbd.role] || 'default'}>
                    {statusData.drbd.role}
                  </Tag>
                </div>
                <div
                  className={`p-3 rounded-lg ${
                    currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                  }`}
                >
                  <div
                    className={`text-xs mb-1 ${
                      currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                    }`}
                  >
                    Disk State
                  </div>
                  <Tag
                    color={statusData.drbd.disk === 'UpToDate' ? 'green' : 'orange'}
                  >
                    {statusData.drbd.disk}
                  </Tag>
                </div>
              </div>

              {statusData.drbd.peers && statusData.drbd.peers.length > 0 && (
                <div className="mt-4">
                  <div
                    className={`text-sm font-medium mb-3 ${
                      currentTheme === 'dark' ? 'text-slate-300' : 'text-slate-700'
                    }`}
                  >
                    Connection States
                  </div>
                  <div className="space-y-2">
                    {statusData.drbd.peers.map((peer: any, idx: number) => (
                      <div
                        key={idx}
                        className={`flex items-center justify-between p-3 rounded-lg ${
                          currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                        }`}
                      >
                        <div className="flex items-center gap-3">
                          <div
                            className="font-medium"
                            style={{ color: ACCENT_COLORS.blue }}
                          >
                            {peer.name}
                          </div>
                          <Tag
                            color={peer.connection === 'Connected' ? 'green' : 'orange'}
                          >
                            {peer.connection || 'Unknown'}
                          </Tag>
                          {peer.replication && (
                            <Tag color="blue">{peer.replication}</Tag>
                          )}
                        </div>
                        {peer.sync_percent !== undefined && (
                          <Progress
                            percent={Math.round(peer.sync_percent)}
                            status={peer.sync_percent >= 100 ? 'success' : 'active'}
                            style={{ width: '150px' }}
                            size="small"
                          />
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </Card>
          )}

          {/* Service Status */}
          {statusData.service_statuses && statusData.service_statuses.length > 0 && (
            <Card
              title={
                <div className="flex items-center gap-3">
                  <div
                    className="w-10 h-10 rounded-xl flex items-center justify-center"
                    style={{
                      background: `linear-gradient(135deg, ${ACCENT_COLORS.mint}30, ${ACCENT_COLORS.cyan}30)`,
                    }}
                  >
                    <CheckCircleOutlined
                      className="text-lg"
                      style={{ color: ACCENT_COLORS.mint }}
                    />
                  </div>
                  <span>Service Status</span>
                </div>
              }
              className="shadow-sm"
              style={{
                borderColor: currentTheme === 'dark' ? '#334155' : '#e2e8f0',
              }}
            >
              <div className="space-y-3">
                {statusData.service_statuses.map((svc: any, idx: number) => (
                  <div
                    key={idx}
                    className={`flex items-center justify-between p-4 rounded-xl ${
                      currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                    }`}
                  >
                    <div className="flex-1">
                      <div className="font-medium text-base mb-1">{svc.name}</div>
                      <div
                        className={`text-sm ${
                          currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                        }`}
                      >
                        {svc.state}
                        {svc.enabled !== undefined && (
                          <span className="ml-2">
                            ({svc.enabled ? 'enabled' : 'disabled'})
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-3">
                      {svc.active ? (
                        <CheckCircleOutlined
                          className="text-2xl"
                          style={{ color: ACCENT_COLORS.mint }}
                        />
                      ) : (
                        <ExclamationCircleOutlined
                          className="text-2xl"
                          style={{ color: ACCENT_COLORS.pink }}
                        />
                      )}
                      <Tag
                        color={svc.active ? 'green' : 'red'}
                        className="text-base px-3 py-1"
                      >
                        {svc.active ? 'Active' : 'Inactive'}
                      </Tag>
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          )}

          {/* Configured Nodes */}
          {statusData.configured_nodes && statusData.configured_nodes.length > 0 && (
            <Card
              title={
                <div className="flex items-center gap-3">
                  <div
                    className="w-10 h-10 rounded-xl flex items-center justify-center"
                    style={{
                      background: `linear-gradient(135deg, ${ACCENT_COLORS.purple}30, ${ACCENT_COLORS.purple}20)`,
                    }}
                  >
                    <FileTextOutlined
                      className="text-lg"
                      style={{ color: ACCENT_COLORS.purple }}
                    />
                  </div>
                  <span>Configured Nodes</span>
                </div>
              }
              className="shadow-sm"
              style={{
                borderColor: currentTheme === 'dark' ? '#334155' : '#e2e8f0',
              }}
            >
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                {statusData.configured_nodes.map((node: any, idx: number) => (
                  <div
                    key={idx}
                    className={`flex items-center justify-between p-4 rounded-xl ${
                      currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
                    }`}
                  >
                    <div className="flex-1">
                      <div className="font-medium text-base mb-1">{node.hostname}</div>
                      <div
                        className={`text-sm ${
                          currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                        }`}
                      >
                        {node.ip}
                      </div>
                    </div>
                    {node.peer_role && (
                      <Tag color={roleColor[node.peer_role] || 'default'} className="text-base px-3 py-1">
                        {node.peer_role}
                      </Tag>
                    )}
                  </div>
                ))}
              </div>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}
