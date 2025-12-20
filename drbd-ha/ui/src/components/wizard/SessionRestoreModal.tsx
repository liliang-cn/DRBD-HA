import { Button, Card, List, Modal, Space, Typography } from 'antd';
import type { WizardSession } from '@/api';

const { Title, Text } = Typography;

interface SessionRestoreModalProps {
  visible: boolean;
  sessions: WizardSession[];
  mode: 'service' | 'storage';
  onLoadSession: (sessionId: string) => void;
  onStartNew: () => void;
  onCancel: () => void;
  loading?: boolean;
}

export const SessionRestoreModal: React.FC<SessionRestoreModalProps> = ({
  visible,
  sessions,
  mode,
  onLoadSession,
  onStartNew,
  onCancel,
  loading = false,
}) => {
  const getStepName = (step: number): string => {
    const stepNames = ['Nodes', 'Storage', 'HA Config', 'Preview', 'Activate'];
    return stepNames[step] || `Step ${step}`;
  };

  const formatTimeAgo = (updatedAt: string): string => {
    try {
      const date = new Date(updatedAt);
      const now = new Date();
      const diffMs = now.getTime() - date.getTime();
      const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
      const diffDays = Math.floor(diffHours / 24);

      if (diffDays > 0) {
        return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`;
      } else if (diffHours > 0) {
        return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`;
      } else {
        return 'Less than an hour ago';
      }
    } catch {
      return 'Unknown time';
    }
  };

  return (
    <Modal
      title="Resume Previous Session"
      open={visible}
      onCancel={onCancel}
      footer={[
        <Button key="cancel" onClick={onCancel} disabled={loading}>
          Cancel
        </Button>,
        <Button
          key="new"
          type="primary"
          onClick={onStartNew}
          disabled={loading}
        >
          Start New Configuration
        </Button>,
      ]}
      width={600}
      destroyOnClose
    >
      <Space direction="vertical" size="middle" style={{ width: '100%' }}>
        <div>
          <Title level={4}>
            {mode === 'service' ? 'HA Service' : 'Storage Sharing'} Wizard
          </Title>
          <Text type="secondary">
            Found {sessions.length} previous{' '}
            {sessions.length === 1 ? 'session' : 'sessions'}. Select one to
            continue where you left off, or start a new configuration.
          </Text>
        </div>

        {sessions.length > 0 ? (
          <List
            grid={{ gutter: 16, column: 1 }}
            dataSource={sessions}
            renderItem={(session) => (
              <List.Item>
                <Card
                  hoverable
                  size="small"
                  onClick={() => onLoadSession(session.id)}
                  style={{ cursor: 'pointer' }}
                >
                  <Space direction="vertical" style={{ width: '100%' }}>
                    <div
                      style={{
                        display: 'flex',
                        justifyContent: 'space-between',
                        alignItems: 'center',
                      }}
                    >
                      <div>
                        <Text strong>Session {session.id.slice(-8)}</Text>
                        <br />
                        <Text code copyable style={{ fontSize: '12px' }}>
                          {session.id}
                        </Text>
                      </div>
                      <Text type="secondary">
                        {formatTimeAgo(session.updated_at)}
                      </Text>
                    </div>

                    <div>
                      <Text>Current Step: </Text>
                      <Text strong>{getStepName(session.current_step)}</Text>
                    </div>

                    {Object.keys(session.step_data).length > 0 && (
                      <div>
                        <Text type="secondary">
                          Saved data for {Object.keys(session.step_data).length}{' '}
                          step
                          {Object.keys(session.step_data).length > 1 ? 's' : ''}
                        </Text>
                      </div>
                    )}
                  </Space>
                </Card>
              </List.Item>
            )}
          />
        ) : (
          <Card>
            <Text type="secondary">
              No previous sessions found for {mode} wizard.
            </Text>
          </Card>
        )}
      </Space>
    </Modal>
  );
};
