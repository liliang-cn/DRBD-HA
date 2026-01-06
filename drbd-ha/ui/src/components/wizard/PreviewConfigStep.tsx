import { Card, Input, Tabs, Typography } from 'antd';

const { Title, Paragraph, Text } = Typography;

interface PreviewConfigStepProps {
  configContent: string | null;
  drbdConfigContent?: string | null;
}

export function PreviewConfigStep({
  configContent,
  drbdConfigContent,
}: PreviewConfigStepProps) {
  const tabItems = [
    {
      key: 'drbd',
      label: (
        <span>
          <strong>DRBD</strong> Configuration
        </span>
      ),
      children: (
        <div className="space-y-4">
          <Paragraph>
            Below is the generated <code>DRBD</code> resource configuration
            file.
          </Paragraph>
          <Input.TextArea
            value={drbdConfigContent || 'No configuration generated yet.'}
            autoSize={{ minRows: 15, maxRows: 30 }}
            readOnly
            style={{ fontFamily: 'monospace' }}
          />
          <Paragraph type="secondary" className="mt-4">
            This file is deployed to <code>/etc/drbd.d/</code> on all cluster
            nodes.
          </Paragraph>
        </div>
      ),
    },
    {
      key: 'reactor',
      label: (
        <span>
          <strong>drbd-reactor</strong> Configuration
        </span>
      ),
      children: (
        <div className="space-y-4">
          <Paragraph>
            Below is the generated <code>drbd-reactor</code> promoter
            configuration file.
          </Paragraph>
          <Input.TextArea
            value={configContent || 'No configuration generated yet.'}
            autoSize={{ minRows: 15, maxRows: 30 }}
            readOnly
            style={{ fontFamily: 'monospace' }}
          />
          <Paragraph type="secondary" className="mt-4">
            This file will be deployed to{' '}
            <code>/etc/drbd-reactor.d/{configContent ? '*.toml' : ''}</code> on
            all cluster nodes.
          </Paragraph>
        </div>
      ),
    },
  ];

  return (
    <Card title="Configuration Preview" className="w-full">
      <Paragraph className="mb-4">
        Review the generated configurations before activating the HA profile.
      </Paragraph>
      <Tabs defaultActiveKey="drbd" items={tabItems} />
    </Card>
  );
}
