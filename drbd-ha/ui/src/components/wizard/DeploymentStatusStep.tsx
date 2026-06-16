import {
  AlertCircle,
  CheckCircle2,
  FileText,
  Loader2,
  RefreshCw,
  Zap,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { haProfilesApi } from '@/api';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { Result } from '@/components/ui/result';
import { cn } from '@/lib/utils';
import { useThemeStore } from '@/stores/theme';
import { ACCENT_COLORS } from '@/theme/colors';
import type { HaProfileStatus } from '@/types';
import { formatJsonForDisplay } from '@/utils/json';

// Map legacy Tag colors to tailwind classes.
const tagColorClass: Record<string, string> = {
  green: 'bg-green-500/15 text-green-600 dark:text-green-400',
  blue: 'bg-blue-500/15 text-blue-600 dark:text-blue-400',
  red: 'bg-red-500/15 text-red-600 dark:text-red-400',
  orange: 'bg-orange-500/15 text-orange-600 dark:text-orange-400',
  default: 'bg-muted text-muted-foreground',
};

function Tag({
  color = 'default',
  className,
  children,
}: {
  color?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium',
        tagColorClass[color] ?? tagColorClass.default,
        className,
      )}
    >
      {children}
    </span>
  );
}

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

// Local card wrapper matching the prior Card look used here.
function PanelCard({
  title,
  className,
  style,
  children,
}: {
  title?: React.ReactNode;
  className?: string;
  style?: React.CSSProperties;
  children: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        'rounded-xl border bg-card text-card-foreground',
        className,
      )}
      style={style}
    >
      {title && (
        <div className="border-b border-border px-6 py-4 font-semibold">
          {title}
        </div>
      )}
      <div className="p-6">{children}</div>
    </div>
  );
}

export function DeploymentStatusStep({
  profileId,
  profileName,
  onDone,
}: DeploymentStatusStepProps) {
  const [loading, setLoading] = useState(true);
  const [statusData, setStatusData] = useState<HaProfileStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [shouldPoll, setShouldPoll] = useState(true);
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

      // Stop polling if status is "active"
      if (status.status === 'active') {
        setShouldPoll(false);
      }
    } catch (err) {
      const errMsg = (err as { message: string }).message;
      setError(errMsg);
      toast.error(errMsg);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!shouldPoll) {
      return; // Don't poll if we've reached active state
    }

    // Poll every 3 seconds
    const interval = setInterval(() => {
      fetchStatus();
    }, 3000);

    // Initial fetch
    fetchStatus();

    return () => clearInterval(interval);
  }, [
    shouldPoll, // Initial fetch
    fetchStatus,
  ]);

  const headerTitle = (
    <h3 className="text-2xl font-bold">
      <span
        className="bg-clip-text text-transparent"
        style={{
          backgroundImage: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
        }}
      >
        Status
      </span>
    </h3>
  );

  if (loading) {
    return (
      <div className="h-full flex flex-col">
        <div className="mb-6">{headerTitle}</div>
        <div className="flex-1 flex flex-col items-center justify-center">
          <div
            className="w-20 h-20 rounded-2xl flex items-center justify-center mb-6"
            style={{
              background: `linear-gradient(135deg, ${ACCENT_COLORS.sky}20, ${ACCENT_COLORS.blue}20)`,
            }}
          >
            <Loader2
              className="h-10 w-10 animate-spin"
              style={{ color: ACCENT_COLORS.sky }}
            />
          </div>
          <span className="text-lg">Checking deployment status...</span>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="h-full flex flex-col">
        <div className="mb-6">{headerTitle}</div>
        <div className="flex-1 flex items-center justify-center">
          <Result
            status="error"
            title="Failed to Check Deployment Status"
            subTitle={error}
            extra={
              <div className="flex items-center gap-3">
                <Button onClick={fetchStatus}>
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Retry
                </Button>
                <Button variant="outline" onClick={onDone}>
                  Go to Dashboard
                </Button>
              </div>
            }
          />
        </div>
      </div>
    );
  }

  const cellBg = currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50';
  const subText = currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500';
  const borderColor = currentTheme === 'dark' ? '#334155' : '#e2e8f0';

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        {headerTitle}
        <Button variant="outline" onClick={fetchStatus}>
          <RefreshCw className="mr-2 h-4 w-4" />
          Refresh
        </Button>
      </div>

      {statusData && (
        <div className="flex-1 overflow-y-auto">
          {/* Success/Warning Message */}
          {statusData.status === 'active' &&
          statusData.service_statuses &&
          statusData.service_statuses.length > 0 &&
          statusData.service_statuses.every((s) => s.active) ? (
            <PanelCard
              className="shadow-sm border-l-4"
              style={{
                borderLeftColor: ACCENT_COLORS.mint,
                borderColor,
                marginBottom: '24px',
              }}
            >
              <div className="flex items-start gap-4">
                <div
                  className="w-16 h-16 rounded-2xl flex items-center justify-center shrink-0"
                  style={{
                    background: `linear-gradient(135deg, ${ACCENT_COLORS.mint}30, ${ACCENT_COLORS.cyan}30)`,
                  }}
                >
                  <CheckCircle2
                    className="h-8 w-8"
                    style={{ color: ACCENT_COLORS.mint }}
                  />
                </div>
                <div className="flex-1">
                  <h3 className="text-xl font-bold mb-2">
                    Deployment Successful!
                  </h3>
                  <p className={cn('text-base', subText)}>
                    HA profile '{profileName}' is active and all services are
                    running on {statusData.active_node || 'the local node'}.
                  </p>
                  <div className="mt-4">
                    <Button size="lg" onClick={onDone}>
                      Go to Dashboard
                    </Button>
                  </div>
                </div>
              </div>
            </PanelCard>
          ) : statusData.status !== 'active' ? (
            <PanelCard
              className="shadow-sm border-l-4"
              style={{
                borderLeftColor: ACCENT_COLORS.gold,
                borderColor,
                marginBottom: '24px',
              }}
            >
              <div className="flex items-start gap-4">
                <div
                  className="w-16 h-16 rounded-2xl flex items-center justify-center shrink-0"
                  style={{
                    background: `linear-gradient(135deg, ${ACCENT_COLORS.gold}30, ${ACCENT_COLORS.orange}30)`,
                  }}
                >
                  <AlertCircle
                    className="h-8 w-8"
                    style={{ color: ACCENT_COLORS.gold }}
                  />
                </div>
                <div className="flex-1">
                  <h3 className="text-xl font-bold mb-2">
                    Deployment Completed
                  </h3>
                  <p className={cn('text-base', subText)}>
                    HA profile '{profileName}' has been created, but the status
                    is '{statusData.status}'. Check the details below for more
                    information.
                  </p>
                  <div className="mt-4">
                    <div className="flex items-center gap-3">
                      <Button onClick={fetchStatus}>Refresh Status</Button>
                      <Button variant="outline" onClick={onDone}>
                        Go to Dashboard
                      </Button>
                    </div>
                  </div>
                </div>
              </div>
            </PanelCard>
          ) : null}

          {/* Status Overview */}
          <PanelCard
            className="shadow-sm"
            style={{ borderColor, marginBottom: '24px' }}
          >
            <div className="flex items-center gap-3 mb-4">
              <div
                className="w-10 h-10 rounded-xl flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
                }}
              >
                <FileText className="h-5 w-5 text-white" />
              </div>
              <h4 className="text-lg font-semibold">Status Overview</h4>
            </div>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div className={cn('p-4 rounded-xl', cellBg)}>
                <div className={cn('text-sm mb-2', subText)}>
                  Profile Status
                </div>
                <Tag
                  color={statusColor[statusData.status] || 'default'}
                  className="text-base px-3 py-1"
                >
                  {statusData.status?.toUpperCase()}
                </Tag>
              </div>
              <div className={cn('p-4 rounded-xl', cellBg)}>
                <div className={cn('text-sm mb-2', subText)}>Active Node</div>
                <div className="text-base font-semibold">
                  {statusData.active_node || '-'}
                </div>
              </div>
              <div className={cn('p-4 rounded-xl', cellBg)}>
                <div className={cn('text-sm mb-2', subText)}>
                  All Services Active
                </div>
                {statusData.service_statuses &&
                statusData.service_statuses.length > 0 ? (
                  statusData.service_statuses.every((s) => s.active) ? (
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
              <div className={cn('p-4 rounded-xl', cellBg)}>
                <div className={cn('text-sm mb-2', subText)}>DRBD Reactor</div>
                <Tag
                  color={statusData.config?.reactor_running ? 'green' : 'red'}
                  className="text-base px-3 py-1"
                >
                  {statusData.config?.reactor_running ? 'Running' : 'Stopped'}
                </Tag>
              </div>
            </div>
          </PanelCard>

          {/* DRBD Status */}
          {statusData.drbd && (
            <PanelCard
              className="shadow-sm"
              style={{ borderColor, marginBottom: '24px' }}
              title={
                <div className="flex items-center gap-3">
                  <div
                    className="w-10 h-10 rounded-xl flex items-center justify-center"
                    style={{
                      background: `linear-gradient(135deg, ${ACCENT_COLORS.sky}30, ${ACCENT_COLORS.blue}30)`,
                    }}
                  >
                    <Zap
                      className="h-5 w-5"
                      style={{ color: ACCENT_COLORS.sky }}
                    />
                  </div>
                  <span>DRBD Resource Status</span>
                </div>
              }
            >
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className={cn('p-3 rounded-lg', cellBg)}>
                  <div className={cn('text-xs mb-1', subText)}>Resource</div>
                  <div className="font-medium">{statusData.drbd.resource}</div>
                </div>
                {statusData.drbd_device && (
                  <div className={cn('p-3 rounded-lg', cellBg)}>
                    <div className={cn('text-xs mb-1', subText)}>
                      DRBD Device
                    </div>
                    <div className="font-medium">{statusData.drbd_device}</div>
                  </div>
                )}
                <div className={cn('p-3 rounded-lg', cellBg)}>
                  <div className={cn('text-xs mb-1', subText)}>Role</div>
                  <Tag color={roleColor[statusData.drbd.role] || 'default'}>
                    {statusData.drbd.role}
                  </Tag>
                </div>
                <div className={cn('p-3 rounded-lg', cellBg)}>
                  <div className={cn('text-xs mb-1', subText)}>Disk State</div>
                  <Tag
                    color={
                      statusData.drbd.disk === 'UpToDate' ? 'green' : 'orange'
                    }
                  >
                    {statusData.drbd.disk}
                  </Tag>
                </div>
              </div>

              {statusData.drbd.peers && statusData.drbd.peers.length > 0 && (
                <div className="mt-4">
                  <div
                    className={cn(
                      'text-sm font-medium mb-3',
                      currentTheme === 'dark'
                        ? 'text-slate-300'
                        : 'text-slate-700',
                    )}
                  >
                    Connection States
                  </div>
                  <div className="space-y-2">
                    {statusData.drbd.peers.map((peer, idx: number) => (
                      <div
                        key={idx}
                        className={cn(
                          'flex items-center justify-between p-3 rounded-lg',
                          cellBg,
                        )}
                      >
                        <div className="flex items-center gap-3">
                          <div
                            className="font-medium"
                            style={{ color: ACCENT_COLORS.blue }}
                          >
                            {peer.name}
                          </div>
                          <Tag
                            color={
                              peer.connection === 'Connected'
                                ? 'green'
                                : 'orange'
                            }
                          >
                            {peer.connection || 'Unknown'}
                          </Tag>
                          {peer.replication && (
                            <Tag color="blue">{peer.replication}</Tag>
                          )}
                        </div>
                        {peer.sync_percent !== undefined && (
                          <Progress
                            value={Math.round(peer.sync_percent)}
                            style={{ width: '150px' }}
                          />
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </PanelCard>
          )}

          {/* DRBD Reactor Status JSON */}
          {statusData.reactor_status_raw && (
            <PanelCard
              title={
                <div className="flex items-center gap-3">
                  <div
                    className="w-10 h-10 rounded-xl flex items-center justify-center"
                    style={{
                      background: `linear-gradient(135deg, ${ACCENT_COLORS.orange}30, ${ACCENT_COLORS.gold}30)`,
                    }}
                  >
                    <FileText
                      className="h-5 w-5"
                      style={{ color: ACCENT_COLORS.orange }}
                    />
                  </div>
                  <span>DRBD Reactor Status JSON</span>
                </div>
              }
              className="shadow-sm"
              style={{ borderColor, marginBottom: '24px' }}
            >
              <pre
                className={cn(
                  'p-4 rounded-lg overflow-x-auto text-xs font-mono',
                  currentTheme === 'dark'
                    ? 'bg-slate-900 text-slate-300'
                    : 'bg-white text-slate-700',
                )}
                style={{
                  border: `1px solid ${borderColor}`,
                  maxHeight: '300px',
                  overflowY: 'auto',
                }}
              >
                {formatJsonForDisplay(statusData.reactor_status_raw)}
              </pre>
            </PanelCard>
          )}

          {/* Service Status */}
          {statusData.service_statuses &&
            statusData.service_statuses.length > 0 && (
              <PanelCard
                title={
                  <div className="flex items-center gap-3">
                    <div
                      className="w-10 h-10 rounded-xl flex items-center justify-center"
                      style={{
                        background: `linear-gradient(135deg, ${ACCENT_COLORS.mint}30, ${ACCENT_COLORS.cyan}30)`,
                      }}
                    >
                      <CheckCircle2
                        className="h-5 w-5"
                        style={{ color: ACCENT_COLORS.mint }}
                      />
                    </div>
                    <span>Service Status</span>
                  </div>
                }
                className="shadow-sm"
                style={{ borderColor, marginBottom: '24px' }}
              >
                <div className="space-y-3">
                  {statusData.service_statuses.map((svc, idx: number) => (
                    <div
                      key={idx}
                      className={cn(
                        'flex items-center justify-between p-4 rounded-xl',
                        cellBg,
                      )}
                    >
                      <div className="flex-1">
                        <div className="font-medium text-base mb-1">
                          {svc.name}
                        </div>
                        <div className={cn('text-sm', subText)}>
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
                          <CheckCircle2
                            className="h-6 w-6"
                            style={{ color: ACCENT_COLORS.mint }}
                          />
                        ) : (
                          <AlertCircle
                            className="h-6 w-6"
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
              </PanelCard>
            )}

          {/* Configured Nodes */}
          {statusData.configured_nodes &&
            statusData.configured_nodes.length > 0 && (
              <PanelCard
                title={
                  <div className="flex items-center gap-3">
                    <div
                      className="w-10 h-10 rounded-xl flex items-center justify-center"
                      style={{
                        background: `linear-gradient(135deg, ${ACCENT_COLORS.purple}30, ${ACCENT_COLORS.purple}20)`,
                      }}
                    >
                      <FileText
                        className="h-5 w-5"
                        style={{ color: ACCENT_COLORS.purple }}
                      />
                    </div>
                    <span>Configured Nodes</span>
                  </div>
                }
                className="shadow-sm"
                style={{ borderColor, marginBottom: '24px' }}
              >
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {statusData.configured_nodes.map((node, idx: number) => (
                    <div
                      key={idx}
                      className={cn(
                        'flex items-center justify-between p-4 rounded-xl',
                        cellBg,
                      )}
                    >
                      <div className="flex-1">
                        <div className="font-medium text-base mb-1">
                          {node.hostname}
                        </div>
                        <div className={cn('text-sm', subText)}>{node.ip}</div>
                      </div>
                      {node.peer_role && (
                        <Tag
                          color={roleColor[node.peer_role] || 'default'}
                          className="text-base px-3 py-1"
                        >
                          {node.peer_role}
                        </Tag>
                      )}
                    </div>
                  ))}
                </div>
              </PanelCard>
            )}
        </div>
      )}
    </div>
  );
}
