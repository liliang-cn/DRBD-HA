import { useState } from 'react';
import { Outlet, useLocation, useNavigate } from 'react-router-dom';
import { Layout, Menu, Badge, theme } from 'antd';
import {
  DashboardOutlined,
  ClusterOutlined,
  DatabaseOutlined,
  SafetyCertificateOutlined,
  FileTextOutlined,
} from '@ant-design/icons';
import { useSSE } from '@/hooks/useSSE';

const { Header, Sider, Content } = Layout;

const menuItems = [
  { key: '/dashboard', icon: <DashboardOutlined />, label: 'HA Status' },
  { key: '/nodes', icon: <ClusterOutlined />, label: 'Nodes' },
  { key: '/storage', icon: <DatabaseOutlined />, label: 'Storage' },
  // { key: '/resources', icon: <DatabaseOutlined />, label: 'Resources' },
  {
    key: '/ha-profiles',
    icon: <SafetyCertificateOutlined />,
    label: 'HA Profiles',
  },
  { key: '/logs', icon: <FileTextOutlined />, label: 'Logs' },
  { key: 'api-docs', icon: <FileTextOutlined />, label: 'API Docs' },
];

export function MainLayout() {
  const [collapsed, setCollapsed] = useState(false);
  const location = useLocation();
  const navigate = useNavigate();
  const { connected } = useSSE();
  const { token } = theme.useToken();

  const handleMenuClick = ({ key }: { key: string }) => {
    if (key === 'api-docs') {
      window.open('/swagger-ui/', '_blank');
    } else {
      navigate(key);
    }
  };

  return (
    <Layout className="min-h-screen">
      <Sider
        collapsible
        collapsed={collapsed}
        onCollapse={setCollapsed}
        theme="light"
        className="border-r border-gray-200"
      >
        <div className="h-16 flex items-center justify-center border-b border-gray-200">
          <span className={`font-bold text-lg ${collapsed ? 'hidden' : ''}`}>
            DRBD HA
          </span>
          {collapsed && <DatabaseOutlined className="text-xl" />}
        </div>
        <Menu
          mode="inline"
          selectedKeys={[location.pathname]}
          items={menuItems}
          onClick={handleMenuClick}
          className="border-r-0"
        />
      </Sider>
      <Layout style={{ background: '#f5f5f5' }}>
        <Header
          className="flex items-center justify-between px-6"
          style={{ background: token.colorBgContainer }}
        >
          <h1 className="text-lg font-medium">DRBD HA</h1>
          <div className="flex items-center gap-4">
            <Badge
              status={connected ? 'success' : 'error'}
              text={connected ? 'Connected' : 'Disconnected'}
            />
          </div>
        </Header>
        <Content
          className="m-3 p-6 bg-white rounded-lg"
          style={{ minHeight: 'calc(100vh - 64px - 24px)' }}
        >
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  );
}
