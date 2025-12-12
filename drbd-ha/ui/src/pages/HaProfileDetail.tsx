import {
  CheckCircleOutlined,
  CloseCircleOutlined,
  LoadingOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import { Button, Card, Descriptions, Result, Table, Tag, message } from 'antd';
import { useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { haProfilesApi } from '@/api';
import { useHaProfilesStore } from '@/stores/ha-profiles';
import type { HaProfile, HaProfileStatusResponse } from '@/types';

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

export function HaProfileDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { profiles } = useHaProfilesStore();
  const [profile, setProfile] = useState<HaProfile | null>(null);
  const [status, setStatus] = useState<HaProfileStatusResponse | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // Try to find profile in store first
    if (profiles.length > 0 && id) {
      const found = profiles.find((p) => p.id === id);
      if (found) setProfile(found);
    }
  }, [profiles, id]);

  const fetchStatus = async () => {
    if (!id) return;
    setLoading(true);
    try {
      // If profile not in store (e.g. direct load), fetch it
      if (!profile) {
        const fetchedProfile = await haProfilesApi.get(id);
        setProfile(fetchedProfile);
      }
      const fetchedStatus = await haProfilesApi.getStatus(id);
      setStatus(fetchedStatus);
    } catch (err) {
      message.error((err as { message: string }).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchStatus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  if (loading && !profile) {
    return (
      <div className="flex justify-center items-center h-screen">
        <LoadingOutlined className="text-4xl text-blue-500" />
      </div>
    );
  }

  if (!profile && !loading) {
    return (
      <Result
        status="404"
        title="Profile Not Found"
        subTitle="Sorry, the profile you visited does not exist."
        extra={
          <Button type="primary" onClick={() => navigate('/')}>
            Back Home
          </Button>
        }
      />
    );
  }

  return (
    <div className="space-y-6 max-w-7xl mx-auto px-4 py-6">
      <div className="flex justify-between items-center">
        <div>
          <h1 className="text-2xl font-bold">{profile?.name}</h1>
          <p className="text-gray-500">
            {profile?.ha_type.toUpperCase()} HA Profile
          </p>
        </div>
        <div className="space-x-2">
          <Button icon={<ReloadOutlined />} onClick={fetchStatus}>
            Refresh Status
          </Button>
          <Button onClick={() => navigate('/')}>Back</Button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Basic Info */}
        <Card title="Overview" size="small">
          <Descriptions bordered column={1}>
            <Descriptions.Item label="ID">{profile?.id}</Descriptions.Item>
            <Descriptions.Item label="Type">
              <Tag>{(profile?.ha_type || 'generic').toUpperCase()}</Tag>
            </Descriptions.Item>
            <Descriptions.Item label="Status">
              {status ? (
                <Tag color={statusColor[status.status]}>
                  {status.status.toUpperCase()}
                </Tag>
              ) : (
                <Tag>Unknown</Tag>
              )}
            </Descriptions.Item>
            <Descriptions.Item label="Active Node">
              {status?.active_node || 'N/A'}
            </Descriptions.Item>
            <Descriptions.Item label="Resource Name">
              {profile?.resource_name}
            </Descriptions.Item>
            <Descriptions.Item label="Mount Point">
              {profile?.mount_point}
            </Descriptions.Item>
            <Descriptions.Item label="File System">
              {profile?.fs_type}
            </Descriptions.Item>
            <Descriptions.Item label="VIP">
              {profile?.vip ? (
                <div className="flex items-center gap-2">
                  <Tag color="green" icon={<CheckCircleOutlined />}>
                    Enabled
                  </Tag>
                  <span>
                    {profile.vip.address} ({profile.vip.interface})
                  </span>
                  {status?.vip_active && <Tag color="green">Active</Tag>}
                </div>
              ) : (
                <Tag color="default" icon={<CloseCircleOutlined />}>
                  Disabled
                </Tag>
              )}
            </Descriptions.Item>
          </Descriptions>
        </Card>

        {/* DRBD Status */}
        <Card title="DRBD Status" size="small">
          {status?.drbd ? (
            <div className="space-y-4">
              <Descriptions bordered column={1}>
                <Descriptions.Item label="Resource">
                  {status.drbd.resource}
                </Descriptions.Item>
                <Descriptions.Item label="Local Role">
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

              <h4 className="font-semibold mt-4 mb-2">Peers</h4>
              {status.drbd.peers.map((peer) => (
                <Descriptions
                  key={peer.name}
                  title={peer.name}
                  bordered
                  column={1}
                  size="small"
                  className="mb-2"
                >
                  <Descriptions.Item label="Role">
                    <Tag color={roleColor[peer.role]}>{peer.role}</Tag>
                  </Descriptions.Item>
                  <Descriptions.Item label="Disk State">
                    {peer.peer_disk}
                  </Descriptions.Item>
                  <Descriptions.Item label="Connection">
                    {peer.connection}
                  </Descriptions.Item>
                  <Descriptions.Item label="Replication">
                    {peer.replication}
                  </Descriptions.Item>
                </Descriptions>
              ))}
            </div>
          ) : (
            <div className="text-gray-400 text-center py-8">
              No DRBD status available
            </div>
          )}
        </Card>

        {/* Protocol Specific Configs */}
        {profile?.ha_type === 'nfs' && profile.nfs && (
          <Card title="NFS Configuration" size="small">
            <Descriptions bordered column={1}>
              <Descriptions.Item label="Export Path">
                {profile.nfs.export_path}
              </Descriptions.Item>
              <Descriptions.Item label="Allowed Networks">
                {profile.nfs.allowed_networks.join(', ')}
              </Descriptions.Item>
              <Descriptions.Item label="Options">
                {profile.nfs.options}
              </Descriptions.Item>
            </Descriptions>
          </Card>
        )}

        {profile?.ha_type === 'iscsi' && profile.iscsi && (
          <Card title="iSCSI Configuration" size="small">
            <Descriptions bordered column={1}>
              <Descriptions.Item label="Target IQN">
                {profile.iscsi.iqn}
              </Descriptions.Item>
              <Descriptions.Item label="Allowed Initiators">
                {profile.iscsi.allowed_initiators.length > 0
                  ? profile.iscsi.allowed_initiators.join(', ')
                  : 'All'}
              </Descriptions.Item>
            </Descriptions>
          </Card>
        )}

        {profile?.ha_type === 'nvmeof' && profile.nvmeof && (
          <Card title="NVMe-oF Configuration" size="small">
            <Descriptions bordered column={1}>
              <Descriptions.Item label="Target NQN">
                {profile.nvmeof.nqn}
              </Descriptions.Item>
              <Descriptions.Item label="Fabric Type">
                {profile.nvmeof.fabric_type.toUpperCase()}
              </Descriptions.Item>
              <Descriptions.Item label="Port">
                {profile.nvmeof.trsvcid}
              </Descriptions.Item>
              <Descriptions.Item label="Allowed NQNs">
                 {profile.nvmeof.allowed_nqns.length > 0
                  ? profile.nvmeof.allowed_nqns.join(', ')
                  : 'All'}
              </Descriptions.Item>
            </Descriptions>
          </Card>
        )}
      </div>

      {/* Services Table */}
      <Card title="Managed Services" size="small">
        <Table
          dataSource={status?.service_statuses || []}
          columns={[
            { title: 'Service Name', dataIndex: 'name', key: 'name' },
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
            { title: 'Systemd State', dataIndex: 'state', key: 'state' },
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
      
       {/* Configuration Visibility */}
       <Card title="System Configuration" size="small">
          <Descriptions bordered column={2}>
              <Descriptions.Item label="Promoter Config Exists">
                  {status?.config.promoter_config_exists ? <Tag color="green">Yes</Tag> : <Tag color="red">No</Tag>}
              </Descriptions.Item>
              <Descriptions.Item label="Promoter Config Path">
                  {status?.config.promoter_config_path}
              </Descriptions.Item>
              <Descriptions.Item label="Reactor Service Running">
                  {status?.config.reactor_running ? <Tag color="green">Yes</Tag> : <Tag color="red">No</Tag>}
              </Descriptions.Item>
          </Descriptions>
           {status?.reactor_status_raw && (
              <div className="mt-4">
                  <h4 className="font-semibold mb-2">Raw Reactor Status</h4>
                  <pre className="bg-gray-100 p-2 rounded text-xs overflow-auto max-h-60">
                      {status.reactor_status_raw}
                  </pre>
              </div>
          )}
      </Card>
    </div>
  );
}
