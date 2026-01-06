import {
  CheckCircleOutlined,
  DeleteOutlined,
  EditOutlined,
  ExclamationCircleOutlined,
  FileTextOutlined,
  LoadingOutlined,
  PauseCircleOutlined,
  PlusOutlined,
  StopOutlined,
  ThunderboltOutlined,
} from '@ant-design/icons';
import {
  Button,
  Checkbox,
  Modal,
  message,
  Popconfirm,
  Space,
  Table,
  Tag,
  Typography,
  Tooltip,
} from 'antd';
import { useNavigate } from 'react-router-dom';
import gsap from 'gsap';
import { useEffect, useRef, useState } from 'react';
import { haProfilesApi } from '@/api';
import { ImportProfilesModal } from '@/components/ha/ImportProfilesModal';
import { useHaProfilesStore } from '@/stores/ha-profiles';
import { useNotificationsStore } from '@/stores/notifications';
import { useResourcesStore } from '@/stores/resources';
import { useThemeStore } from '@/stores/theme';
import { ACCENT_COLORS } from '@/theme/colors';
import type { HaProfile, HaProfileStatus } from '@/types';

const { Title, Text } = Typography;

const statusColor: Record<string, string> = {
  active: 'green',
  standby: 'blue',
  stopped: 'default',
  error: 'red',
  unknown: 'default',
};

const statusIcons: Record<string, React.ReactNode> = {
  active: <CheckCircleOutlined className="text-green-500" />,
  standby: <PauseCircleOutlined className="text-orange-500" />,
  stopped: <ExclamationCircleOutlined className="text-gray-400" />,
  error: <ExclamationCircleOutlined className="text-red-500" />,
  unknown: <LoadingOutlined className="text-gray-400" />,
};

export function HaProfiles() {
  const navigate = useNavigate();
  const { profiles, loading, fetch } = useHaProfilesStore();
  const { fetch: fetchResources } = useResourcesStore();
  const { theme: currentTheme } = useThemeStore();
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

  // GSAP Refs
  const headerRef = useRef<HTMLDivElement>(null);
  const statsRef = useRef<HTMLDivElement>(null);
  const tableRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef<(HTMLDivElement | null)[]>([]);

  useEffect(() => {
    fetch();
    fetchResources();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fetch, fetchResources]); // Only run on mount

  // GSAP Animations - only run once after data is loaded
  const hasAnimated = useRef(false);
  const [isDataLoaded, setIsDataLoaded] = useState(false);

  useEffect(() => {
    // Mark data as loaded when profiles are fetched
    if (profiles.length > 0) {
      setIsDataLoaded(true);
    }
  }, [profiles]);

  // Preload resource agents metadata after profiles are loaded
  useEffect(() => {
    if (isDataLoaded) {
      // Fetch resource agents metadata to populate localStorage cache
      // This ensures OCF Agent Editor loads instantly when navigated to
      haProfilesApi.getAllResourceAgents().then(result => {
        const cacheKey = 'ha-profiles:resource-agents';
        localStorage.setItem(cacheKey, JSON.stringify(result));
        localStorage.setItem(`${cacheKey}:timestamp`, Date.now().toString());
      }).catch(err => {
        console.warn('Failed to preload resource agents:', err);
      });
    }
  }, [isDataLoaded]);

  useEffect(() => {
    if (hasAnimated.current || !isDataLoaded) return;

    // Use requestAnimationFrame to ensure DOM is ready
    requestAnimationFrame(() => {
      if (hasAnimated.current) return;
      hasAnimated.current = true;

      const tl = gsap.timeline({ defaults: { ease: 'power3.out' } });

      if (headerRef.current) {
        tl.fromTo(
          headerRef.current,
          { y: -30, opacity: 0 },
          { y: 0, opacity: 1, duration: 0.6 },
        );
      }

      if (statsRef.current) {
        tl.fromTo(
          statsRef.current,
          { y: 30, opacity: 0 },
          { y: 0, opacity: 1, duration: 0.5 },
          '-=0.3',
        );
      }

      if (tableRef.current) {
        tl.fromTo(
          tableRef.current,
          { y: 30, opacity: 0 },
          { y: 0, opacity: 1, duration: 0.5 },
          '-=0.3',
        );
      }

      // Animate stat cards with stagger
      cardRefs.current.forEach((card, index) => {
        if (card) {
          gsap.fromTo(
            card,
            { y: 20, opacity: 0, scale: 0.95 },
            {
              y: 0,
              opacity: 1,
              scale: 1,
              duration: 0.4,
              delay: index * 0.1,
              ease: 'back.out(1.7)',
            },
          );
        }
      });
    });
  }, [isDataLoaded]);

  // Scroll logs to bottom
  useEffect(() => {
    if (progressModalOpen) {
      logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [progressModalOpen]);

  // Listen for deletion progress
  useEffect(() => {
    if (!deletingProfileName || !progressModalOpen) return;

    const relevantProgress = (progressEvents || []).filter((p) => {
      if (p.resource === deletingProfileName) {
        return true;
      }

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
      const sortedProgress = relevantProgress.sort((a, b) =>
        a.operation_id.localeCompare(b.operation_id),
      );

      const newSteps = sortedProgress
        .map((p) => ({
          message: p.message,
          done: p.completed,
        }))
        .filter((s) => s.message);

      if (newSteps.length > 0) {
        setDeletionProgressSteps(newSteps);
      }

      sortedProgress.forEach((progress) => {
        const messageId = `${progress.operation_id}_${progress.progress}_${progress.message}`;

        if (progress.message && !processedMessageIds.current.has(messageId)) {
          setDeletionLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] ${progress.message}`,
          ]);
          processedMessageIds.current.add(messageId);
        }

        if (progress.completed && progress.success === false) {
          setDeletionLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] ERROR: ${progress.message}`,
          ]);
          setDeleting(false);
        } else if (progress.completed && progress.success === true) {
          setDeletionLogs((prev) => [
            ...prev,
            `[${new Date().toLocaleTimeString()}] Deletion completed successfully.`,
          ]);
          setDeleting(false);
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
  }, [
    progressEvents,
    deletingProfileName,
    progressModalOpen,
    fetch,
    fetchResources,
  ]);

  const openDeleteModal = (profile: HaProfile) => {
    setProfileToDelete(profile);
    setDeleteResource(true);
    setDeleteModalOpen(true);
  };

  const handleDelete = async () => {
    if (!profileToDelete) return;

    setDeletingProfileName(profileToDelete.name);
    setDeletionLogs([]);
    setDeletionProgressSteps([]);
    processedMessageIds.current.clear();
    setDeleteModalOpen(false);
    setProgressModalOpen(true);
    setDeleting(true);

    setDeletionLogs([
      `[${new Date().toLocaleTimeString()}] Requesting deletion of ${profileToDelete.name}...`,
    ]);

    try {
      await haProfilesApi.delete(profileToDelete.id, deleteResource);

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

  const handleDisableNode = async (profileId: string, node: string) => {
    try {
      await haProfilesApi.disableNode(profileId, node);
      message.success(`Profile disabled on ${node}`);
      fetch();

      // Refresh status if this profile is expanded
      if (expandedProfileId === profileId) {
        setStatusLoading((prev) => ({ ...prev, [profileId]: true }));
        try {
          const status = await haProfilesApi.getStatus(profileId);
          setProfileStatuses((prev) => ({ ...prev, [profileId]: status }));
        } catch (err) {
          message.error((err as { message: string }).message);
        } finally {
          setStatusLoading((prev) => ({ ...prev, [profileId]: false }));
        }
      }
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  const handleEnableNode = async (profileId: string, node: string) => {
    try {
      await haProfilesApi.enableNode(profileId, node);
      message.success(`Profile enabled on ${node}`);
      fetch();

      // Refresh status if this profile is expanded
      if (expandedProfileId === profileId) {
        setStatusLoading((prev) => ({ ...prev, [profileId]: true }));
        try {
          const status = await haProfilesApi.getStatus(profileId);
          setProfileStatuses((prev) => ({ ...prev, [profileId]: status }));
        } catch (err) {
          message.error((err as { message: string }).message);
        } finally {
          setStatusLoading((prev) => ({ ...prev, [profileId]: false }));
        }
      }
    } catch (err) {
      message.error((err as { message: string }).message);
    }
  };

  // Calculate stats
  const activeCount = profiles.filter((p) => p.status === 'active').length;
  const standbyCount = profiles.filter((p) => p.status === 'standby').length;
  const errorCount = profiles.filter((p) => p.status === 'error').length;

  // Card colors using theme palette
  const cardColors = [
    { from: ACCENT_COLORS.orange, to: ACCENT_COLORS.gold }, // Total
    { from: ACCENT_COLORS.mint, to: ACCENT_COLORS.cyan }, // Active
    { from: ACCENT_COLORS.sky, to: ACCENT_COLORS.blue }, // Standby
    { from: ACCENT_COLORS.pink, to: '#ff4757' }, // Errors
  ];

  const columns = [
    {
      title: 'Profile',
      dataIndex: 'name',
      key: 'name',
      render: (name: string, record: HaProfile) => (
        <div className="flex items-center gap-3">
          <div className="relative">
            <div
              className={`w-10 h-10 rounded-lg flex items-center justify-center ${
                record.status === 'active'
                  ? 'bg-gradient-to-br from-green-400 to-green-600'
                  : record.status === 'error'
                    ? 'bg-gradient-to-br from-red-400 to-red-600'
                    : currentTheme === 'dark'
                      ? 'bg-gradient-to-br from-slate-600 to-slate-700'
                      : 'bg-gradient-to-br from-slate-300 to-slate-400'
              }`}
            >
              <ThunderboltOutlined className="text-white text-lg" />
            </div>
            {record.status === 'active' && (
              <div className="absolute -top-1 -right-1 w-3 h-3 bg-green-500 rounded-full border-2 border-white animate-pulse" />
            )}
          </div>
          <div>
            <div
              className={`font-semibold ${currentTheme === 'dark' ? 'text-slate-100' : 'text-slate-800'}`}
            >
              {name}
            </div>
            <div
              className={`text-xs ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
            >
              {record.resource_name}
            </div>
          </div>
        </div>
      ),
    },
    {
      title: 'Type',
      dataIndex: 'ha_type',
      key: 'ha_type',
      render: (_: unknown, record: HaProfile) => (
        <Space>
          <Tag className="px-3 py-1 rounded-full font-medium">
            {(record.ha_type || 'generic').toUpperCase()}
          </Tag>
          {record.is_builtin_plugin && (
            <Tag color="cyan" className="px-2 py-1 rounded text-xs">
              Built-in
            </Tag>
          )}
        </Space>
      ),
    },
    {
      title: 'Services',
      key: 'services',
      render: (_: unknown, record: HaProfile) => (
        <Space wrap>
          {record.promoter.services.slice(0, 2).map((s) => (
            <Tag key={s}>{s}</Tag>
          ))}
          {record.promoter.services.length > 2 && (
            <Tag>+{record.promoter.services.length - 2}</Tag>
          )}
        </Space>
      ),
    },
    {
      title: 'Service IP',
      key: 'vip',
      render: (_: unknown, record: HaProfile) => {
        // Built-in plugins don't have VIP
        if (record.is_builtin_plugin) {
          return (
            <span
              className={
                currentTheme === 'dark' ? 'text-slate-500' : 'text-slate-400'
              }
            >
              N/A
            </span>
          );
        }
        if (record.vip) {
          return (
            <div className="flex items-center gap-2">
              <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
              <span
                className="font-medium"
                style={{ color: ACCENT_COLORS.sky }}
              >
                {record.vip.address}/{record.vip.netmask}
              </span>
            </div>
          );
        }
        return (
          <span
            className={
              currentTheme === 'dark' ? 'text-slate-500' : 'text-slate-400'
            }
          >
            Not Enabled
          </span>
        );
      },
    },
    {
      title: 'Status',
      dataIndex: 'status',
      key: 'status',
      render: (status: string) => (
        <div className="flex items-center gap-2">
          {statusIcons[status] || statusIcons.unknown}
          <Tag
            color={statusColor[status] || 'default'}
            className="px-3 py-1 rounded-full font-medium"
          >
            {status.toUpperCase()}
          </Tag>
        </div>
      ),
    },
    {
      title: 'Active Node',
      dataIndex: 'active_node',
      key: 'active_node',
      render: (node: string | undefined, record: HaProfile) => {
        const isActive = record.status === 'active';
        return (
          <Space>
            {node ? (
              <div className="flex items-center gap-2">
                <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse" />
                <span className="font-medium">{node}</span>
              </div>
            ) : (
              <span
                className={
                  currentTheme === 'dark' ? 'text-slate-500' : 'text-slate-400'
                }
              >
                -
              </span>
            )}
            {isActive && node && !record.is_builtin_plugin && (
              <Popconfirm
                title="Evict Profile"
                description={`Are you sure you want to evict the HA profile from ${node}?`}
                onConfirm={() => handleEvict(record.id)}
                okText="Evict"
                cancelText="Cancel"
                okButtonProps={{ danger: true }}
                icon={<ExclamationCircleOutlined style={{ color: 'red' }} />}
              >
                <Button
                  size="small"
                  danger
                  className="!text-red-600 hover:!bg-red-50 hover:!border-red-300"
                >
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
      render: (_: unknown, record: HaProfile) => (
        <Space>
          {!record.is_builtin_plugin && (
            <>
              <Tooltip title="Edit OCF Agents">
                <Button
                  size="small"
                  type="text"
                  icon={<EditOutlined />}
                  onClick={() => navigate(`/profiles/${record.name}/ocf-edit`)}
                  className="hover:!bg-purple-50"
                />
              </Tooltip>
              <Tooltip title="Delete">
                <Button
                  size="small"
                  type="text"
                  danger
                  icon={<DeleteOutlined />}
                  onClick={() => openDeleteModal(record)}
                  className="hover:!bg-red-50"
                />
              </Tooltip>
            </>
          )}
        </Space>
      ),
    },
  ];

  // Render expanded row content
  const expandedRowRender = (record: HaProfile) => {
    // Built-in plugins don't support detailed status
    if (record.is_builtin_plugin) {
      return (
        <div className="p-6">
          <div
            className={`p-4 rounded-lg text-center ${
              currentTheme === 'dark'
                ? 'bg-slate-700/50 text-slate-400'
                : 'bg-slate-50 text-slate-500'
            }`}
          >
            Built-in plugins do not support detailed status view.
          </div>
        </div>
      );
    }

    const status = profileStatuses[record.id];
    const isLoading = statusLoading[record.id];

    if (isLoading) {
      return (
        <div className="flex justify-center items-center p-12">
          <div className="flex flex-col items-center gap-4">
            <div
              className="w-16 h-16 rounded-2xl flex items-center justify-center"
              style={{
                background: `linear-gradient(135deg, ${ACCENT_COLORS.sky}20, ${ACCENT_COLORS.blue}20)`,
              }}
            >
              <LoadingOutlined
                className="text-3xl"
                style={{ color: ACCENT_COLORS.sky }}
              />
            </div>
            <div className="flex gap-1">
              {['T', 'h', 'i', 'n', 'k', 'i', 'n', 'g', '.', '.', '.'].map(
                (letter, idx) => (
                  <span
                    key={idx}
                    className={`text-sm font-medium inline-block ${
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }`}
                    style={{
                      animation: 'wave-bounce 1.4s ease-in-out infinite',
                      animationDelay: `${idx * 0.08}s`,
                    }}
                  >
                    {letter}
                  </span>
                ),
              )}
            </div>
            <style>{`
              @keyframes wave-bounce {
                0%, 100% {
                  transform: translateY(0);
                }
                50% {
                  transform: translateY(-8px);
                }
              }
            `}</style>
          </div>
        </div>
      );
    }

    if (!status) {
      return (
        <div className="p-6">
          <div
            className={`p-4 rounded-lg text-center ${
              currentTheme === 'dark'
                ? 'bg-slate-700/50 text-slate-400'
                : 'bg-slate-50 text-slate-500'
            }`}
          >
            Status information not available.
          </div>
        </div>
      );
    }

    return (
      <div className="p-6 space-y-4">
        {/* Status Banner */}
        <div
          className="p-5 rounded-xl border-l-4"
          style={{
            borderLeftColor:
              record.status === 'active'
                ? ACCENT_COLORS.mint
                : ACCENT_COLORS.gold,
            background:
              record.status === 'active'
                ? `linear-gradient(135deg, ${ACCENT_COLORS.mint}10, ${ACCENT_COLORS.cyan}10)`
                : `linear-gradient(135deg, ${ACCENT_COLORS.gold}10, ${ACCENT_COLORS.orange}10)`,
            borderColor: currentTheme === 'dark' ? '#334155' : '#e2e8f0',
          }}
        >
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <div
                className="w-12 h-12 rounded-xl flex items-center justify-center"
                style={{
                  background:
                    record.status === 'active'
                      ? `linear-gradient(135deg, ${ACCENT_COLORS.mint}, ${ACCENT_COLORS.cyan})`
                      : `linear-gradient(135deg, ${ACCENT_COLORS.gold}, ${ACCENT_COLORS.orange})`,
                }}
              >
                {record.status === 'active' ? (
                  <CheckCircleOutlined className="text-2xl text-white" />
                ) : (
                  <ExclamationCircleOutlined className="text-2xl text-white" />
                )}
              </div>
              <div>
                <div
                  className={`text-lg font-semibold ${currentTheme === 'dark' ? 'text-white' : 'text-slate-800'}`}
                >
                  Status: <span className="uppercase">{record.status}</span>
                </div>
                <div
                  className={`text-sm ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                >
                  {record.active_node
                    ? `Running on ${record.active_node}`
                    : 'No active node'}
                </div>
              </div>
            </div>
            <div className="flex gap-4">
              {status?.drbd_device && (
                <div className="text-right">
                  <div
                    className={`text-xs mb-1 ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                  >
                    DRBD Device
                  </div>
                  <Tag color="blue">{status.drbd_device}</Tag>
                </div>
              )}
              {status?.vip_active !== null && (
                <div className="text-right">
                  <div
                    className={`text-xs mb-1 ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                  >
                    VIP Status
                  </div>
                  <Tag color={status?.vip_active ? 'green' : 'default'}>
                    {status?.vip_active ? 'Active' : 'Inactive'}
                  </Tag>
                </div>
              )}
              {status?.service_statuses &&
                status?.service_statuses.length > 0 && (
                  <div className="text-right">
                    <div
                      className={`text-xs mb-1 ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                    >
                      Services
                    </div>
                    <div className="flex flex-wrap gap-1 justify-end">
                      {status?.service_statuses.map((s: any, idx: number) => (
                        <Tag
                          key={idx}
                          color={s.active ? 'green' : 'red'}
                          className="mb-0"
                        >
                          {s.name}
                        </Tag>
                      ))}
                    </div>
                  </div>
                )}
            </div>
          </div>
        </div>

        {/* DRBD Status Section */}
        {status?.drbd && (
          <div
            className={`p-5 rounded-xl border ${
              currentTheme === 'dark'
                ? 'bg-slate-700/50 border-slate-600'
                : 'bg-slate-50 border-slate-200'
            }`}
          >
            <div className="flex items-center gap-3 mb-4">
              <div
                className="w-10 h-10 rounded-xl flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.sky}30, ${ACCENT_COLORS.blue}30)`,
                }}
              >
                <ThunderboltOutlined
                  className="text-xl"
                  style={{ color: ACCENT_COLORS.sky }}
                />
              </div>
              <div>
                <div
                  className={`text-lg font-semibold ${currentTheme === 'dark' ? 'text-white' : 'text-slate-800'}`}
                >
                  DRBD Resource: {status.drbd.resource}
                </div>
                <div
                  className={`text-sm ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                >
                  {status.drbd.open ? 'Device Open' : 'Device Closed'}
                </div>
              </div>
            </div>

            <div className="grid grid-cols-2 md:grid-cols-5 gap-4 mb-4">
              <div
                className={`p-3 rounded-lg text-center ${
                  currentTheme === 'dark' ? 'bg-slate-700' : 'bg-white'
                }`}
              >
                <div
                  className={`text-xs mb-1 ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                >
                  Role
                </div>
                <Tag color={status.drbd.role === 'Primary' ? 'green' : 'blue'}>
                  {status.drbd.role}
                </Tag>
              </div>
              <div
                className={`p-3 rounded-lg text-center ${
                  currentTheme === 'dark' ? 'bg-slate-700' : 'bg-white'
                }`}
              >
                <div
                  className={`text-xs mb-1 ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                >
                  Disk State
                </div>
                <Tag
                  color={status.drbd.disk === 'UpToDate' ? 'green' : 'orange'}
                >
                  {status.drbd.disk}
                </Tag>
              </div>
              {status.drbd.peers && status.drbd.peers.length > 0 && (
                <div
                  className={`p-3 rounded-lg text-center ${
                    currentTheme === 'dark' ? 'bg-slate-700' : 'bg-white'
                  }`}
                >
                  <div
                    className={`text-xs mb-1 ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                  >
                    Peers Connected
                  </div>
                  <div
                    className="text-lg font-semibold"
                    style={{ color: ACCENT_COLORS.blue }}
                  >
                    {
                      status.drbd.peers.filter(
                        (p: any) => p.connection === 'Connected',
                      ).length
                    }
                    /{status.drbd.peers.length}
                  </div>
                </div>
              )}
            </div>

            {/* DRBD Peers/Connections */}
            {status.drbd.peers && status.drbd.peers.length > 0 && (
              <div>
                <div
                  className={`text-sm font-semibold mb-3 ${currentTheme === 'dark' ? 'text-slate-300' : 'text-slate-700'}`}
                >
                  Peer Connections
                </div>
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                  {status.drbd.peers.map((peer: any, idx: number) => (
                    <div
                      key={idx}
                      className={`p-4 rounded-xl ${
                        currentTheme === 'dark' ? 'bg-slate-700' : 'bg-white'
                      }`}
                    >
                      <div className="flex items-center justify-between mb-3">
                        <div
                          className="font-semibold"
                          style={{ color: ACCENT_COLORS.blue }}
                        >
                          {peer.name}
                        </div>
                        <Tag
                          color={peer.role === 'Primary' ? 'green' : 'blue'}
                          size="small"
                        >
                          {peer.role}
                        </Tag>
                      </div>
                      <div className="space-y-2 text-sm">
                        <div className="flex justify-between items-center">
                          <span
                            className={
                              currentTheme === 'dark'
                                ? 'text-slate-400'
                                : 'text-slate-500'
                            }
                          >
                            Connection:
                          </span>
                          <Tag
                            color={
                              peer.connection === 'Connected'
                                ? 'green'
                                : 'orange'
                            }
                            size="small"
                          >
                            {peer.connection || 'Unknown'}
                          </Tag>
                        </div>
                        <div className="flex justify-between items-center">
                          <span
                            className={
                              currentTheme === 'dark'
                                ? 'text-slate-400'
                                : 'text-slate-500'
                            }
                          >
                            Replication:
                          </span>
                          <Tag color="blue" size="small">
                            {peer.replication || 'N/A'}
                          </Tag>
                        </div>
                        <div className="flex justify-between items-center">
                          <span
                            className={
                              currentTheme === 'dark'
                                ? 'text-slate-400'
                                : 'text-slate-500'
                            }
                          >
                            Peer Disk:
                          </span>
                          <Tag
                            color={
                              peer.peer_disk === 'UpToDate' ? 'green' : 'orange'
                            }
                            size="small"
                          >
                            {peer.peer_disk}
                          </Tag>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        )}

        {/* Configured Nodes */}
        {status?.configured_nodes && status.configured_nodes.length > 0 && (
          <div
            className={`p-5 rounded-xl border ${
              currentTheme === 'dark'
                ? 'bg-slate-700/50 border-slate-600'
                : 'bg-slate-50 border-slate-200'
            }`}
          >
            <div className="flex items-center gap-3 mb-4">
              <div
                className="w-10 h-10 rounded-xl flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.purple}30, ${ACCENT_COLORS.purple}20)`,
                }}
              >
                <FileTextOutlined
                  className="text-xl"
                  style={{ color: ACCENT_COLORS.purple }}
                />
              </div>
              <div
                className={`text-lg font-semibold ${currentTheme === 'dark' ? 'text-white' : 'text-slate-800'}`}
              >
                Configured Nodes ({status.configured_nodes.length})
              </div>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
              {status.configured_nodes.map((node: any, idx: number) => {
                const isActive =
                  node.peer_role === 'Primary' ||
                  node.hostname === status.active_node;
                const isNodeDisabled = node.disabled === true;

                return (
                  <div
                    key={idx}
                    className={`p-4 rounded-xl border-2 transition-all relative ${
                      isNodeDisabled
                        ? 'opacity-60 bg-gray-100 dark:bg-slate-800 border-gray-300 dark:border-slate-600'
                        : isActive
                          ? 'border-green-500 shadow-lg shadow-green-500/20'
                          : currentTheme === 'dark'
                            ? 'bg-slate-700 border-transparent'
                            : 'bg-white border-transparent'
                    }`}
                    style={
                      !isNodeDisabled && isActive
                        ? {
                            background:
                              currentTheme === 'dark'
                                ? `linear-gradient(135deg, ${ACCENT_COLORS.mint}15, ${ACCENT_COLORS.cyan}15)`
                                : `linear-gradient(135deg, ${ACCENT_COLORS.mint}10, ${ACCENT_COLORS.cyan}10)`,
                          }
                        : {}
                    }
                  >
                    {/* Disable/Enable button in top-right corner */}
                    <div className="absolute top-3 right-3">
                      {isNodeDisabled ? (
                        <Button
                          size="small"
                          type="primary"
                          icon={<CheckCircleOutlined />}
                          onClick={() =>
                            handleEnableNode(record.id, node.hostname)
                          }
                          className="!bg-green-500 !border-green-500 hover:!bg-green-600"
                          title={`Enable ${node.hostname}`}
                        />
                      ) : (
                        <Popconfirm
                          title="Disable Profile"
                          description={`Disable this profile on ${node.hostname}?`}
                          onConfirm={() =>
                            handleDisableNode(record.id, node.hostname)
                          }
                          okText="Disable"
                          cancelText="Cancel"
                          okButtonProps={{ danger: true }}
                        >
                          <Button
                            size="small"
                            danger
                            icon={<StopOutlined />}
                            title={`Disable ${node.hostname}`}
                          />
                        </Popconfirm>
                      )}
                    </div>

                    <div className="mb-6">
                      <div className="flex items-center gap-2 mb-1">
                        <div
                          className="font-semibold text-lg"
                          style={{
                            color: isNodeDisabled
                              ? '#9ca3af'
                              : ACCENT_COLORS.purple,
                          }}
                        >
                          {node.hostname}
                        </div>
                        {isActive && !isNodeDisabled && (
                          <Tag color="green" size="small">
                            Active
                          </Tag>
                        )}
                        {isNodeDisabled && (
                          <Tag color="default" size="small">
                            Disabled
                          </Tag>
                        )}
                      </div>
                      {node.peer_role && (
                        <Tag
                          color={
                            node.peer_role === 'Primary' ? 'green' : 'blue'
                          }
                          size="small"
                        >
                          {node.peer_role}
                        </Tag>
                      )}
                    </div>

                    <div
                      className={`text-sm ${isNodeDisabled ? 'text-gray-400' : currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                    >
                      {node.ip}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Resource & Configuration Grid */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {/* Resource Info */}
          <div
            className={`p-4 rounded-xl ${
              currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
            }`}
          >
            <div className="flex items-center gap-2 mb-3">
              <div
                className="w-8 h-8 rounded-lg flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.blue}30, ${ACCENT_COLORS.sky}30)`,
                }}
              >
                <ThunderboltOutlined
                  className="text-base"
                  style={{ color: ACCENT_COLORS.blue }}
                />
              </div>
              <span className="font-semibold">Resource</span>
            </div>
            <div className="space-y-2 text-sm">
              <div>
                <div
                  className={
                    currentTheme === 'dark'
                      ? 'text-slate-400'
                      : 'text-slate-500'
                  }
                >
                  Name
                </div>
                <div className="font-medium">{record.resource_name}</div>
              </div>
              <div>
                <div
                  className={
                    currentTheme === 'dark'
                      ? 'text-slate-400'
                      : 'text-slate-500'
                  }
                >
                  Mount Point
                </div>
                <div className="font-medium">{record.mount_point}</div>
              </div>
              <div>
                <div
                  className={
                    currentTheme === 'dark'
                      ? 'text-slate-400'
                      : 'text-slate-500'
                  }
                >
                  Filesystem
                </div>
                <Tag>{record.fs_type}</Tag>
              </div>
            </div>
          </div>

          {/* Configuration */}
          <div
            className={`p-4 rounded-xl ${
              currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
            }`}
          >
            <div className="flex items-center gap-2 mb-3">
              <div
                className="w-8 h-8 rounded-lg flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.orange}30, ${ACCENT_COLORS.gold}30)`,
                }}
              >
                <FileTextOutlined
                  className="text-base"
                  style={{ color: ACCENT_COLORS.orange }}
                />
              </div>
              <span className="font-semibold">Configuration</span>
            </div>
            <div className="space-y-2 text-sm">
              <div>
                <div
                  className={
                    currentTheme === 'dark'
                      ? 'text-slate-400'
                      : 'text-slate-500'
                  }
                >
                  Type
                </div>
                <Tag>{(record.ha_type || 'generic').toUpperCase()}</Tag>
              </div>
              <div>
                <div
                  className={
                    currentTheme === 'dark'
                      ? 'text-slate-400'
                      : 'text-slate-500'
                  }
                >
                  Mount Strategy
                </div>
                <div className="font-medium">{record.mount_strategy}</div>
              </div>
              {record.vip && (
                <div>
                  <div
                    className={
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }
                  >
                    Virtual IP
                  </div>
                  <div
                    className="font-medium"
                    style={{ color: ACCENT_COLORS.sky }}
                  >
                    {record.vip.address}/{record.vip.netmask}
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Promoter Services */}
          <div
            className={`p-4 rounded-xl ${
              currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
            }`}
          >
            <div className="flex items-center gap-2 mb-3">
              <div
                className="w-8 h-8 rounded-lg flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.mint}30, ${ACCENT_COLORS.cyan}30)`,
                }}
              >
                <CheckCircleOutlined
                  className="text-base"
                  style={{ color: ACCENT_COLORS.mint }}
                />
              </div>
              <span className="font-semibold">Services</span>
            </div>
            <div className="space-y-2">
              <div>
                <div
                  className={`text-xs ${currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'}`}
                >
                  Managed Services
                </div>
                <div className="flex flex-wrap gap-1 mt-1">
                  {record.promoter.services.map((s) => (
                    <Tag key={s} color="blue" size="small">
                      {s}
                    </Tag>
                  ))}
                </div>
              </div>
              <div className="grid grid-cols-2 gap-2 text-xs">
                <div>
                  <div
                    className={
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }
                  >
                    Stop on Demote
                  </div>
                  <Tag
                    color={record.promoter.stop_on_demote ? 'green' : 'default'}
                    size="small"
                  >
                    {record.promoter.stop_on_demote ? 'Yes' : 'No'}
                  </Tag>
                </div>
                <div>
                  <div
                    className={
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }
                  >
                    On Failure
                  </div>
                  <div className="font-medium">
                    {record.promoter.on_demote_failure}
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Advanced Promoter Settings */}
        {(record.promoter.preferred_nodes?.length ||
          record.promoter.on_quorum_loss ||
          record.nfs ||
          record.iscsi ||
          record.nvmeof) && (
          <div
            className={`p-5 rounded-xl border ${
              currentTheme === 'dark'
                ? 'bg-slate-700/50 border-slate-600'
                : 'bg-slate-50 border-slate-200'
            }`}
          >
            <div
              className={`text-lg font-semibold mb-4 ${currentTheme === 'dark' ? 'text-white' : 'text-slate-800'}`}
            >
              Advanced Settings
            </div>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              {record.promoter.preferred_nodes &&
                record.promoter.preferred_nodes.length > 0 && (
                  <div>
                    <div
                      className={
                        currentTheme === 'dark'
                          ? 'text-slate-400'
                          : 'text-slate-500'
                      }
                    >
                      Preferred Nodes
                    </div>
                    <div className="flex flex-wrap gap-1 mt-1">
                      {record.promoter.preferred_nodes.map((node, idx) => (
                        <Tag key={idx} size="small">
                          {node}
                        </Tag>
                      ))}
                    </div>
                  </div>
                )}
              {record.promoter.on_quorum_loss && (
                <div>
                  <div
                    className={
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }
                  >
                    On Quorum Loss
                  </div>
                  <div className="font-medium">
                    {record.promoter.on_quorum_loss}
                  </div>
                </div>
              )}
              {record.nfs && (
                <div>
                  <div
                    className={
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }
                  >
                    NFS Export
                  </div>
                  <div className="font-medium">{record.nfs.export_path}</div>
                </div>
              )}
              {record.iscsi && (
                <div>
                  <div
                    className={
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }
                  >
                    iSCSI Target
                  </div>
                  <div className="font-medium">{record.iscsi.iqn}</div>
                </div>
              )}
              {record.nvmeof && (
                <div>
                  <div
                    className={
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }
                  >
                    NVMe-oF
                  </div>
                  <div className="font-medium">{record.nvmeof.nqn}</div>
                </div>
              )}
            </div>
          </div>
        )}

        {/* Generated Units */}
        {status?.config && (
          <div
            className={`p-4 rounded-xl ${
              currentTheme === 'dark' ? 'bg-slate-700/50' : 'bg-slate-50'
            }`}
          >
            <div className="flex items-center gap-2 mb-3">
              <div
                className="w-8 h-8 rounded-lg flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.gold}30, ${ACCENT_COLORS.orange}30)`,
                }}
              >
                <FileTextOutlined
                  className="text-base"
                  style={{ color: ACCENT_COLORS.gold }}
                />
              </div>
              <span className="font-semibold">System Configuration</span>
            </div>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              <div>
                <div
                  className={
                    currentTheme === 'dark'
                      ? 'text-slate-400'
                      : 'text-slate-500'
                  }
                >
                  Promoter Config
                </div>
                <Tag
                  color={
                    status.config.promoter_config_exists ? 'green' : 'default'
                  }
                >
                  {status.config.promoter_config_exists ? 'Exists' : 'Missing'}
                </Tag>
              </div>
              <div>
                <div
                  className={
                    currentTheme === 'dark'
                      ? 'text-slate-400'
                      : 'text-slate-500'
                  }
                >
                  DRBD Reactor
                </div>
                <Tag color={status.config.reactor_running ? 'green' : 'red'}>
                  {status.config.reactor_running ? 'Running' : 'Stopped'}
                </Tag>
              </div>
              {status.config.promoter_config_path && (
                <div className="col-span-2">
                  <div
                    className={
                      currentTheme === 'dark'
                        ? 'text-slate-400'
                        : 'text-slate-500'
                    }
                  >
                    Config Path
                  </div>
                  <div className="font-mono text-xs">
                    {status.config.promoter_config_path}
                  </div>
                </div>
              )}
            </div>
          </div>
        )}

        {/* DRBD Reactor Status Raw */}
        {status?.reactor_status_raw && (
          <div
            className={`p-5 rounded-xl border ${
              currentTheme === 'dark'
                ? 'bg-slate-700/50 border-slate-600'
                : 'bg-slate-50 border-slate-200'
            }`}
          >
            <div className="flex items-center gap-3 mb-4">
              <div
                className="w-10 h-10 rounded-xl flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.orange}30, ${ACCENT_COLORS.gold}30)`,
                }}
              >
                <FileTextOutlined
                  className="text-xl"
                  style={{ color: ACCENT_COLORS.orange }}
                />
              </div>
              <div
                className={`text-lg font-semibold ${currentTheme === 'dark' ? 'text-white' : 'text-slate-800'}`}
              >
                DRBD Reactor Status
              </div>
            </div>
            <pre
              className={`p-4 rounded-lg overflow-x-auto text-xs font-mono ${
                currentTheme === 'dark'
                  ? 'bg-slate-900 text-slate-300'
                  : 'bg-white text-slate-700'
              }`}
              style={{
                border: `1px solid ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
                maxHeight: '400px',
                overflowY: 'auto',
              }}
            >
              {status.reactor_status_raw}
            </pre>
          </div>
        )}

        {/* DRBD Config Raw */}
        {status?.drbd_config_raw && (
          <div
            className={`p-5 rounded-xl border ${
              currentTheme === 'dark'
                ? 'bg-slate-700/50 border-slate-600'
                : 'bg-slate-50 border-slate-200'
            }`}
          >
            <div className="flex items-center gap-3 mb-4">
              <div
                className="w-10 h-10 rounded-xl flex items-center justify-center"
                style={{
                  background: `linear-gradient(135deg, ${ACCENT_COLORS.sky}30, ${ACCENT_COLORS.blue}30)`,
                }}
              >
                <ThunderboltOutlined
                  className="text-xl"
                  style={{ color: ACCENT_COLORS.sky }}
                />
              </div>
              <div
                className={`text-lg font-semibold ${currentTheme === 'dark' ? 'text-white' : 'text-slate-800'}`}
              >
                DRBD Configuration ({record.resource_name}.res)
              </div>
            </div>
            <pre
              className={`p-4 rounded-lg overflow-x-auto text-xs font-mono ${
                currentTheme === 'dark'
                  ? 'bg-slate-900 text-slate-300'
                  : 'bg-white text-slate-700'
              }`}
              style={{
                border: `1px solid ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
                maxHeight: '400px',
                overflowY: 'auto',
              }}
            >
              {status.drbd_config_raw}
            </pre>
          </div>
        )}
      </div>
    );
  };

  return (
    <div
      className={`rounded-xl shadow border p-6 h-[calc(100vh-4rem)] overflow-y-auto ${
        currentTheme === 'dark'
          ? 'bg-slate-800 border-slate-700'
          : 'bg-white border-slate-200'
      }`}
    >
      {/* Show loading state */}
      {loading ? (
        <div className="flex items-center justify-center h-full">
          <div className="flex flex-col items-center gap-4">
            <LoadingOutlined
              className="text-4xl animate-spin"
              style={{ color: ACCENT_COLORS.orange }}
            />
            <div
              className={
                currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
              }
            >
              Loading profiles...
            </div>
          </div>
        </div>
      ) : (
        <div className="space-y-6">
          {/* Header */}
          <div
            ref={headerRef}
            className="flex flex-col md:flex-row justify-between items-start md:items-center gap-4"
          >
            <div>
              <Title level={2} className="!mb-1 !text-2xl">
                <span
                  className="bg-clip-text text-transparent"
                  style={{
                    backgroundImage: `linear-gradient(135deg, ${ACCENT_COLORS.orange}, ${ACCENT_COLORS.gold})`,
                  }}
                >
                  HA Profiles
                </span>
              </Title>
              <Text
                className={
                  currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                }
              >
                Manage your high availability configurations
              </Text>
            </div>
            <Button
              type="primary"
              icon={<PlusOutlined />}
              onClick={() => navigate('/service-ha/create?step=1')}
              className="!h-10 !px-5"
            >
              Create Service HA
            </Button>
          </div>

          {/* Stats Cards */}
          <div ref={statsRef} className="grid grid-cols-1 md:grid-cols-4 gap-4">
            {[
              {
                label: 'Total Profiles',
                value: profiles.length,
                icon: <ThunderboltOutlined className="text-2xl" />,
              },
              {
                label: 'Active',
                value: activeCount,
                icon: <CheckCircleOutlined className="text-2xl" />,
              },
              {
                label: 'Standby',
                value: standbyCount,
                icon: <PauseCircleOutlined className="text-2xl" />,
              },
              {
                label: 'Errors',
                value: errorCount,
                icon: <ExclamationCircleOutlined className="text-2xl" />,
              },
            ].map((stat, index) => (
              <div
                key={index}
                ref={(el) => (cardRefs.current[index] = el)}
                className="rounded-xl p-5 text-white shadow-lg"
                style={{
                  background: `linear-gradient(135deg, ${cardColors[index].from}, ${cardColors[index].to})`,
                  boxShadow: `0 10px 40px -10px ${cardColors[index].from}40`,
                }}
              >
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-white/80 text-sm font-medium">
                      {stat.label}
                    </div>
                    <div className="text-3xl font-bold mt-1">{stat.value}</div>
                  </div>
                  <div className="w-12 h-12 bg-white/20 rounded-xl flex items-center justify-center">
                    {stat.icon}
                  </div>
                </div>
              </div>
            ))}
          </div>

          {/* Table */}
          <div ref={tableRef}>
            <Table
              dataSource={profiles}
              columns={columns}
              rowKey="id"
              pagination={false}
              expandable={{
                expandedRowRender: (record: HaProfile) => {
                  // Built-in plugins don't support expand
                  if (record.is_builtin_plugin) {
                    return null;
                  }
                  return expandedRowRender(record);
                },
                onExpand: (expanded, record) => {
                  // Built-in plugins don't support expand
                  if (record.is_builtin_plugin) {
                    return false;
                  }
                  return handleRowExpand(expanded, record);
                },
                expandedRowKeys: expandedProfileId ? [expandedProfileId] : [],
                rowExpandable: (record: HaProfile) => !record.is_builtin_plugin,
              }}
              className="shadow-lg rounded-xl overflow-hidden"
            />
          </div>
        </div>
      )}

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
          <span className="flex items-center gap-2">
            <ExclamationCircleOutlined style={{ color: '#faad14' }} />
            Delete HA Profile
          </span>
        }
        open={deleteModalOpen}
        onCancel={() => setDeleteModalOpen(false)}
        onOk={handleDelete}
        okText="Delete"
        okButtonProps={{ danger: true, loading: deleting }}
        cancelButtonProps={{ disabled: deleting }}
        className="!rounded-2xl"
      >
        {profileToDelete && (
          <div className="space-y-4">
            <p>
              Are you sure you want to delete the HA profile{' '}
              <strong>{profileToDelete.name}</strong>?
            </p>
            <div
              className={`p-4 rounded-xl border ${
                currentTheme === 'dark'
                  ? 'bg-slate-700 border-slate-600'
                  : 'bg-slate-50 border-slate-200'
              }`}
            >
              <Checkbox
                checked={deleteResource}
                onChange={(e) => setDeleteResource(e.target.checked)}
              >
                Also delete DRBD resource{' '}
                <strong>{profileToDelete.resource_name}</strong>
              </Checkbox>
              <p
                className={`text-sm mt-2 ml-6 ${
                  currentTheme === 'dark' ? 'text-slate-400' : 'text-slate-500'
                }`}
              >
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
          <span className="flex items-center gap-2">
            <LoadingOutlined />
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
              <div
                className={`text-sm font-medium mb-2 ${
                  currentTheme === 'dark' ? 'text-slate-300' : 'text-slate-700'
                }`}
              >
                Deletion Progress:
              </div>
              {deletionProgressSteps.map((step, idx) => (
                <div key={idx} className="flex items-start gap-2 text-sm">
                  {step.done ? (
                    <CheckCircleOutlined className="text-green-500 mt-1 shrink-0" />
                  ) : (
                    <LoadingOutlined className="text-orange-500 mt-1 shrink-0" />
                  )}
                  <span
                    className={
                      step.done
                        ? currentTheme === 'dark'
                          ? 'text-slate-300'
                          : 'text-slate-700'
                        : 'text-orange-600 font-medium'
                    }
                  >
                    {step.message}
                  </span>
                </div>
              ))}
            </div>
          )}

          {/* Logs */}
          <div className="h-[200px] overflow-y-auto bg-slate-900 text-green-400 p-4 rounded-xl font-mono text-xs border border-slate-700">
            {deletionLogs.length === 0 ? (
              <div className="text-slate-500 text-center mt-20">
                Waiting for logs...
              </div>
            ) : (
              deletionLogs.map((log, i) => (
                <div key={i} className="mb-1">
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
