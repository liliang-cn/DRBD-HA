import { Card, Result, Button, Space, Spin, Progress } from 'antd';
import { LoadingOutlined, CheckCircleOutlined } from '@ant-design/icons';

interface ActivationStepProps {
  activationStatus:
    | 'pending'
    | 'creating'
    | 'activating'
    | 'checking'
    | 'success'
    | 'error';
  activationError: string | null;
  progressPercent: number;
  progressSteps: Array<{ message: string; done: boolean }>;
  onRetry?: () => void;
  onDone?: () => void;
}

export function ActivationStep({
  activationStatus,
  activationError,
  progressPercent,
  progressSteps,
  onRetry,
  onDone,
}: ActivationStepProps) {
  return (
    <Card title="Step 4: Activating HA" className="max-w-4xl mx-auto">
      {(activationStatus === 'creating' ||
        activationStatus === 'activating' ||
        activationStatus === 'checking') && (
        <div className="py-6">
          <div className="text-center mb-6">
            <Spin
              indicator={<LoadingOutlined style={{ fontSize: 48 }} spin />}
            />
            <div className="mt-4 text-xl font-medium">
              {activationStatus === 'creating' &&
                'Creating HA Profile & Storage...'}
              {activationStatus === 'activating' && 'Activating HA...'}
              {activationStatus === 'checking' && 'Verifying Services...'}
            </div>
          </div>

          <Progress
            percent={progressPercent}
            status="active"
            strokeColor={{ '0%': '#108ee9', '100%': '#87d068' }}
            className="mb-6"
          />

          <Card size="small" className="bg-gray-50">
            <div className="space-y-2 max-h-64 overflow-y-auto">
              {progressSteps.map((s, idx) => (
                <div key={idx} className="flex items-center gap-2">
                  {s.done ? (
                    <CheckCircleOutlined className="text-green-500" />
                  ) : (
                    <LoadingOutlined className="text-blue-500" spin />
                  )}
                  <span className={s.done ? 'text-gray-500' : 'text-gray-700'}>
                    {s.message}
                  </span>
                </div>
              ))}
            </div>
          </Card>
        </div>
      )}

      {activationStatus === 'success' && (
        <Result
          status="success"
          title="HA Setup Complete!"
          subTitle="All services are running successfully"
          extra={
            <div className="max-w-md mx-auto">
              <Button type="primary" onClick={onDone}>
                Go to Dashboard
              </Button>
            </div>
          }
        />
      )}

      {activationStatus === 'error' && (
        <Result
          status="error"
          title="Activation Failed"
          subTitle={activationError}
          extra={
            <Space>
              <Button type="primary" onClick={onRetry}>
                Retry Activation
              </Button>
              <Button onClick={onDone}>Go to Dashboard</Button>
            </Space>
          }
        />
      )}
    </Card>
  );
}
