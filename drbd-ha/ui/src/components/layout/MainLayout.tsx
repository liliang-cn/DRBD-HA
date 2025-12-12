import { FileTextOutlined } from '@ant-design/icons';
import { Button, Layout, Tooltip, theme } from 'antd';
import { Link, Outlet } from 'react-router-dom';

const { Header, Content } = Layout;

export function MainLayout() {
  const { token } = theme.useToken();

  return (
    <Layout className="min-h-screen">
      <Layout style={{ background: '#f5f5f5' }}>
        <Header
          className="flex items-center justify-between px-6"
          style={{ background: token.colorBgContainer }}
        >
          <div className="flex items-center gap-4">
            <Link to="/" style={{ color: 'inherit', textDecoration: 'none' }}>
              <h1 className="text-lg font-medium m-0">DRBD HA</h1>
            </Link>
          </div>
          <div className="flex items-center gap-4">
            <Tooltip title="API Documentation">
              <Button
                type="text"
                icon={<FileTextOutlined style={{ fontSize: '18px' }} />}
                onClick={() => window.open('/swagger-ui/', '_blank')}
              />
            </Tooltip>
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
