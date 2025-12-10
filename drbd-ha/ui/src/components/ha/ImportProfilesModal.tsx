import { useState, useEffect } from 'react';
import { Modal, Table, Button, message, Tag, Space, Typography } from 'antd';
import { ImportOutlined, ReloadOutlined } from '@ant-design/icons';
import { haProfilesApi } from '@/api';
import type { HaProfile } from '@/types';

interface ImportProfilesModalProps {
  open: boolean;
  onCancel: () => void;
  onSuccess: () => void;
}

export function ImportProfilesModal({
  open,
  onCancel,
  onSuccess,
}: ImportProfilesModalProps) {
  const [loading, setLoading] = useState(false);
  const [importing, setImporting] = useState(false);
  const [profiles, setProfiles] = useState<HaProfile[]>([]);
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);

  const fetchUnmanaged = async () => {
    setLoading(true);
    try {
      const data = await haProfilesApi.getUnmanaged();
      setProfiles(data);
    } catch (err) {
      message.error('Failed to discover profiles');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) {
      fetchUnmanaged();
      setSelectedRowKeys([]);
    }
  }, [open]);

  const handleImport = async () => {
    if (selectedRowKeys.length === 0) return;

    setImporting(true);
    try {
      const names = selectedRowKeys as string[];
      const res = await haProfilesApi.importProfiles(names);

      if (res.imported.length > 0) {
        message.success(
          `Successfully imported ${res.imported.length} profiles`,
        );
      }
      if (res.failed.length > 0) {
        message.warning(`Failed to import: ${res.failed.join(', ')}`);
      }

      onSuccess();
      onCancel();
    } catch (err) {
      message.error('Import failed');
    } finally {
      setImporting(false);
    }
  };

  const columns = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    {
      title: 'Type',
      dataIndex: 'ha_type',
      key: 'ha_type',
      render: (t: string) => <Tag>{(t || 'Generic').toUpperCase()}</Tag>,
    },
    { title: 'Resource', dataIndex: 'resource_name', key: 'resource_name' },
    {
      title: 'Services',
      key: 'services',
      render: (_: unknown, r: HaProfile) => (
        <span className="text-xs text-gray-500">
          {r.promoter.services.join(', ')}
        </span>
      ),
    },
  ];

  return (
    <Modal
      title="Import Existing HA Profiles"
      open={open}
      onCancel={onCancel}
      width={700}
      footer={[
        <Button key="cancel" onClick={onCancel}>
          Cancel
        </Button>,
        <Button
          key="import"
          type="primary"
          icon={<ImportOutlined />}
          loading={importing}
          disabled={selectedRowKeys.length === 0}
          onClick={handleImport}
        >
          Import Selected ({selectedRowKeys.length})
        </Button>,
      ]}
    >
      <div className="mb-4 flex justify-between items-center">
        <Typography.Text type="secondary">
          The following profiles were found in <code>/etc/drbd-reactor.d/</code>{' '}
          but are not managed by the database.
        </Typography.Text>
        <Button
          icon={<ReloadOutlined />}
          onClick={fetchUnmanaged}
          loading={loading}
          size="small"
        >
          Refresh
        </Button>
      </div>

      <Table
        dataSource={profiles}
        columns={columns}
        rowKey="name"
        loading={loading}
        size="small"
        pagination={false}
        rowSelection={{
          selectedRowKeys,
          onChange: (keys) => setSelectedRowKeys(keys),
        }}
        scroll={{ y: 300 }}
      />
    </Modal>
  );
}
