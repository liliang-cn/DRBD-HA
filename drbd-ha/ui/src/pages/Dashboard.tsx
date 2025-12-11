import { useEffect, useState } from 'react';
import {
  Card,
  Row,
  Col,
  Statistic,
  Tag,
  Table,
  Typography,
  Spin,
  Alert,
} from 'antd';
import {
  ClusterOutlined,
  HddOutlined,
  AppstoreOutlined,
  CheckCircleOutlined,
  WarningOutlined,
  CloseCircleOutlined,
  SyncOutlined,
} from '@ant-design/icons';
import { dashboardApi } from '@/api';
import type { DashboardSummary, HaServiceDetail } from '@/types';

const { Text } = Typography;

export function Dashboard() {
  const [summary, setSummary] = useState<DashboardSummary | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchSummary = async () => {
    try {
      const data = await dashboardApi.getSummary();
      setSummary(data);
    } catch (err) {
      console.error('Failed to fetch dashboard summary:', err);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchSummary();
    const interval = setInterval(fetchSummary, 5000);
    return () => clearInterval(interval);
  }, []);

  if (loading && !summary) {
    return (
      <div className="flex items-center justify-center h-96">
        <Spin size="large" />
      </div>
    );
  }

  if (!summary) return null;

  // Cluster Health Status
  const healthStatus = {
    healthy: {
      color: 'success',
      icon: <CheckCircleOutlined />,
      text: 'Healthy',
      message: 'All systems operational',
    },
    warning: {
      color: 'warning',
      icon: <WarningOutlined />,
      text: 'Warning',
      message: 'Some components are degraded',
    },
    critical: {
      color: 'error',
      icon: <CloseCircleOutlined />,
      text: 'Critical',
      message: 'Cluster is in critical state',
    },
  }[summary.health];

  // Table Columns
  const columns = [
    {
      title: 'Service Name',
      dataIndex: 'name',
      key: 'name',
      render: (text: string) => <span className="font-semibold">{text}</span>,
    },
    {
      title: 'Type',
      dataIndex: 'service_type',
      key: 'service_type',
      render: (type: string) => (
        <Tag color="blue">{type || 'Unknown'}</Tag>
      ),
    },
    {
      title: 'Status',
      dataIndex: 'status',
      key: 'status',
      render: (status: string) => {
        const color =
          status === 'active'
            ? 'green'
            : status === 'standby'
            ? 'cyan'
            : 'default';
        return <Tag color={color}>{status.toUpperCase()}</Tag>;
      },
    },
    {
      title: 'Current Node',
      dataIndex: 'active_node',
      key: 'active_node',
      render: (node: string) =>
        node ? (
          <Tag icon={<CheckCircleOutlined />} color="success">
            {node}
          </Tag>
        ) : (
          <Text type="secondary">-</Text>
        ),
    },
    {
      title: 'Cluster Nodes',
      dataIndex: 'nodes',
      key: 'nodes',
      render: (nodes: string[]) => (
        <div className="flex flex-wrap gap-1">
          {nodes && nodes.length > 0 ? (
            nodes.map((n) => <Tag key={n}>{n}</Tag>)
          ) : (
            <Text type="secondary">-</Text>
          )}
        </div>
      ),
    },
    {
      title: 'VIP',
      dataIndex: 'vip',
      key: 'vip',
      render: (vip: string) =>
        vip ? <Text copyable>{vip}</Text> : <Text type="secondary">-</Text>,
    },
    {
      title: 'Export / Info',
      key: 'info',
      render: (_: unknown, record: HaServiceDetail) => {
        if (record.export_path) {
          return (
            <div>
              <span className="text-gray-500 text-xs mr-1">Export:</span>
              <Text copyable>{record.export_path}</Text>
            </div>
          );
        }
        return <Text type="secondary">-</Text>;
      },
    },
  ];

  return (
    <div className="space-y-6">
      {/* Health Status Bar */}
      <Alert
        message={
          <div className="flex items-center gap-2 text-lg">
            {healthStatus.icon}
            <span className="font-bold">
              Cluster Status: {healthStatus.text}
            </span>
          </div>
        }
        description={healthStatus.message}
        type={healthStatus.color as 'success' | 'warning' | 'error'}
        showIcon={false}
        className="border-l-4"
      />

      {/* Key Metrics Cards */}
      <Row gutter={16}>
        <Col span={6}>
          <Card>
            <Statistic
              title="Cluster Nodes"
              value={summary.nodes.online}
              suffix={`/ ${summary.nodes.total}`}
              prefix={<ClusterOutlined />}
              valueStyle={{
                color: summary.nodes.offline > 0 ? '#cf1322' : '#3f8600',
              }}
            />
            <div className="text-xs text-gray-500 mt-2">
              {summary.nodes.offline > 0
                ? `${summary.nodes.offline} Offline`
                : 'All Online'}
            </div>
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic
              title="Storage (LVM)"
              value={summary.storage.total_bytes / 1024 / 1024 / 1024}
              precision={1}
              suffix="GB"
              prefix={<HddOutlined />}
            />
            <div className="text-xs text-gray-500 mt-2">
              Free:{' '}
              {(summary.storage.free_bytes / 1024 / 1024 / 1024).toFixed(1)} GB
              ({summary.storage.pool_count} Pools)
            </div>
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic
              title="HA Profiles"
              value={summary.ha_services.active}
              suffix={`/ ${summary.ha_services.total}`}
              prefix={<AppstoreOutlined />}
              valueStyle={{
                color: summary.ha_services.error > 0 ? '#cf1322' : '#3f8600',
              }}
            />
            <div className="text-xs text-gray-500 mt-2">
              {summary.ha_services.standby} Standby
            </div>
          </Card>
        </Col>
        <Col span={6}>
          <Card>
            <Statistic
              title="DRBD Resources"
              value={summary.resources.healthy}
              suffix={`/ ${summary.resources.total}`}
              prefix={<SyncOutlined spin={summary.resources.degraded > 0} />}
              valueStyle={{
                color: summary.resources.degraded > 0 ? '#faad14' : '#3f8600',
              }}
            />
            <div className="text-xs text-gray-500 mt-2">
              {summary.resources.degraded > 0
                ? `${summary.resources.degraded} Degraded`
                : 'All Healthy'}
            </div>
          </Card>
        </Col>
      </Row>

      {/* HA Clusters Table */}
      <Card title="HA Clusters / Services" className="w-full">
        <Table
          dataSource={summary.ha_service_details}
          columns={columns}
          rowKey="name"
          pagination={false}
          locale={{ emptyText: 'No HA Services configured' }}
        />
      </Card>
    </div>
  );
}