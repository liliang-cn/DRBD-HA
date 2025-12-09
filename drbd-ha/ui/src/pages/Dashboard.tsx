import { useEffect, useState } from "react";
import {
  Card,
  Row,
  Col,
  Statistic,
  Tag,
  List,
  Typography,
  Spin,
  Alert,
  Badge,
} from "antd";
import {
  ClusterOutlined,
  HddOutlined,
  AppstoreOutlined,
  CheckCircleOutlined,
  WarningOutlined,
  CloseCircleOutlined,
  SyncOutlined,
} from "@ant-design/icons";
import { dashboardApi } from "@/api";
import { useNotificationsStore } from "@/stores/notifications";
import type { DashboardSummary, NotificationEvent } from "@/types";

const { Text } = Typography;

export function Dashboard() {
  const [summary, setSummary] = useState<DashboardSummary | null>(null);
  const [loading, setLoading] = useState(true);
  // Live events from SSE; store key is `notifications`, not `events`
  const notifications = useNotificationsStore((s) => s.notifications);

  const fetchSummary = async () => {
    try {
      const data = await dashboardApi.getSummary();
      setSummary(data);
    } catch (err) {
      console.error("Failed to fetch dashboard summary:", err);
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

  // [F01] Cluster Health Traffic Light
  const healthStatus = {
    healthy: {
      color: "success",
      icon: <CheckCircleOutlined />,
      text: "Healthy",
      message: "All systems operational",
    },
    warning: {
      color: "warning",
      icon: <WarningOutlined />,
      text: "Warning",
      message: "Some components are degraded",
    },
    critical: {
      color: "error",
      icon: <CloseCircleOutlined />,
      text: "Critical",
      message: "Cluster is in critical state",
    },
  }[summary.health];

  return (
    <div className="space-y-6">
      {/* [F01] Health Status Bar */}
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
        type={healthStatus.color as "success" | "warning" | "error"}
        showIcon={false}
        className="border-l-4"
      />

      {/* [F03] Key Metrics Cards */}
      <Row gutter={16}>
        <Col span={6}>
          <Card>
            <Statistic
              title="Cluster Nodes"
              value={summary.nodes.online}
              suffix={`/ ${summary.nodes.total}`}
              prefix={<ClusterOutlined />}
              valueStyle={{
                color: summary.nodes.offline > 0 ? "#cf1322" : "#3f8600",
              }}
            />
            <div className="text-xs text-gray-500 mt-2">
              {summary.nodes.offline > 0
                ? `${summary.nodes.offline} Offline`
                : "All Online"}
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
              Free:{" "}
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
                color: summary.ha_services.error > 0 ? "#cf1322" : "#3f8600",
              }}
            />
            <div className="text-xs text-gray-500 mt-2">
              {summary.ha_services.standby} Standby,{" "}
              {summary.ha_services.stopped} Stopped
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
                color: summary.resources.degraded > 0 ? "#faad14" : "#3f8600",
              }}
            />
            <div className="text-xs text-gray-500 mt-2">
              {summary.resources.degraded > 0
                ? `${summary.resources.degraded} Degraded`
                : "All Healthy"}
            </div>
          </Card>
        </Col>
      </Row>

      <Row gutter={16}>
        {/* [F02] Topology Map (Simplified Visualization) */}
        <Col span={16}>
          <Card title="Cluster Topology" className="h-full min-h-[400px]">
            <div className="flex flex-col items-center justify-center h-full space-y-8 p-4">
              <div className="flex justify-center gap-16 w-full">
                {/* Visualize Nodes */}
                {Array.from({ length: summary.nodes.total }).map((_, i) => (
                  <div key={i} className="relative group">
                    <div
                      className={`
                      w-32 h-32 border-2 rounded-lg flex flex-col items-center justify-center bg-gray-50
                      ${
                        i < summary.nodes.online
                          ? "border-green-500 shadow-green-100"
                          : "border-red-500 bg-red-50"
                      }
                      transition-all hover:shadow-lg
                    `}
                    >
                      <ClusterOutlined className="text-3xl mb-2 text-gray-600" />
                      <div className="font-bold">Node {i + 1}</div>
                      <Badge
                        status={i < summary.nodes.online ? "success" : "error"}
                        text={i < summary.nodes.online ? "Online" : "Offline"}
                      />
                    </div>

                    {/* Resources linked to this node (Mockup visualization) */}
                    <div className="absolute -bottom-4 left-1/2 transform -translate-x-1/2 translate-y-full space-y-1">
                      {i === 0 && summary.ha_services.active > 0 && (
                        <Tag color="green">
                          Active Services: {summary.ha_services.active}
                        </Tag>
                      )}
                      {i === 1 && summary.ha_services.standby > 0 && (
                        <Tag color="blue">
                          Standby: {summary.ha_services.standby}
                        </Tag>
                      )}
                    </div>
                  </div>
                ))}
              </div>

              {/* Sync Line */}
              <div className="w-2/3 h-1 bg-gray-200 relative rounded">
                {summary.resources.degraded > 0 ? (
                  <div
                    className="absolute inset-0 bg-yellow-400 animate-pulse rounded"
                    style={{ width: "100%" }}
                  ></div>
                ) : (
                  <div
                    className="absolute inset-0 bg-green-400 rounded"
                    style={{ width: "100%" }}
                  ></div>
                )}
                <div className="absolute -top-6 left-1/2 transform -translate-x-1/2 bg-white px-2 text-xs text-gray-500">
                  DRBD Replication (
                  {summary.resources.degraded > 0
                    ? "Syncing/Degraded"
                    : "Healthy"}
                  )
                </div>
              </div>
            </div>
          </Card>
        </Col>

        {/* [F04] Live Event Log */}
        <Col span={8}>
          <Card
            title="Live Events"
            className="h-full min-h-[400px]"
            bodyStyle={{ padding: 0 }}
          >
            <div className="h-[350px] overflow-y-auto p-4">
              <List
                dataSource={[...notifications].reverse()} // Show newest first
                renderItem={(item: NotificationEvent) => (
                  <List.Item className="border-b border-gray-100 last:border-0">
                    <List.Item.Meta
                      avatar={
                        item.level === "error" ? (
                          <CloseCircleOutlined className="text-red-500" />
                        ) : item.level === "warning" ? (
                          <WarningOutlined className="text-yellow-500" />
                        ) : (
                          <CheckCircleOutlined className="text-blue-500" />
                        )
                      }
                      title={<Text className="text-sm">{item.message}</Text>}
                      description={
                        <div className="flex justify-between text-xs text-gray-400">
                          <span>{item.source}</span>
                          <span>
                            {new Date(
                              item.timestamp * 1000
                            ).toLocaleTimeString()}
                          </span>
                        </div>
                      }
                    />
                  </List.Item>
                )}
                locale={{ emptyText: "No recent events" }}
              />
            </div>
          </Card>
        </Col>
      </Row>

      {/* HA Services Status Row */}
      <Row gutter={16}>
        <Col span={24}>
          <Card title="High Availability Services Status">
            <div className="space-y-3">
              {summary.ha_service_details &&
              summary.ha_service_details.length > 0 ? (
                summary.ha_service_details.map((service) => (
                  <div
                    key={service.name}
                    className="flex items-center justify-between p-3 bg-gray-50 rounded border border-gray-200 hover:bg-gray-100 transition"
                  >
                    <div className="flex items-center gap-3 flex-1">
                      <div
                        className="w-2 h-2 rounded-full"
                        style={{
                          backgroundColor: service.active_node
                            ? "#52c41a"
                            : "#bfbfbf",
                        }}
                      ></div>
                      <div className="flex-1">
                        <div className="font-semibold text-sm">
                          {service.name}
                        </div>
                        <div className="text-xs text-gray-500">
                          {service.active_node
                            ? `Active on ${service.active_node}`
                            : "Not active"}
                        </div>
                      </div>
                    </div>
                    <Tag
                      color={service.active_node ? "green" : "default"}
                      className="text-xs"
                    >
                      {service.status}
                    </Tag>
                  </div>
                ))
              ) : (
                <div className="text-center py-8 text-gray-400">
                  No HA services configured
                </div>
              )}
            </div>
          </Card>
        </Col>
      </Row>
    </div>
  );
}
