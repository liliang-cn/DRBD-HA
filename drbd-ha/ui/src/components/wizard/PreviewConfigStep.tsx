import { Card, Input, Typography } from 'antd';

const { Title, Paragraph } = Typography;

interface PreviewConfigStepProps {
  configContent: string | null;
}

export function PreviewConfigStep({ configContent }: PreviewConfigStepProps) {
  return (
    <Card title="Generated Configuration" className="max-w-4xl mx-auto">
      <Paragraph>
        Below is the generated <code>drbd-reactor</code> configuration file.
      </Paragraph>
      <Input.TextArea
        value={configContent || 'No configuration generated yet.'}
        autoSize={{ minRows: 15, maxRows: 30 }}
        readOnly
        style={{ fontFamily: 'monospace' }}
      />
      <Paragraph type="secondary" className="mt-4">
        This file is deployed to <code>/etc/drbd-reactor.d/</code> on all
        cluster nodes.
      </Paragraph>
    </Card>
  );
}
