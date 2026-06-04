import { CheckCircle2, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Progress } from '@/components/ui/progress';
import { Result } from '@/components/ui/result';

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
    <Card className="w-full">
      <CardHeader>
        <CardTitle>Step 4: Activating HA</CardTitle>
      </CardHeader>
      <CardContent>
        {(activationStatus === 'creating' ||
          activationStatus === 'activating' ||
          activationStatus === 'checking') && (
          <div className="py-6">
            <div className="text-center mb-6">
              <Loader2 className="mx-auto h-12 w-12 animate-spin text-primary" />
              <div className="mt-4 text-xl font-medium">
                {activationStatus === 'creating' &&
                  'Creating HA Profile & Storage...'}
                {activationStatus === 'activating' && 'Activating HA...'}
                {activationStatus === 'checking' && 'Verifying Services...'}
              </div>
            </div>

            <Progress value={progressPercent} className="mb-6" />

            <div className="rounded-lg border border-border bg-muted p-3">
              <div className="space-y-2 max-h-64 overflow-y-auto">
                {progressSteps.map((s, idx) => (
                  <div key={idx} className="flex items-center gap-2">
                    {s.done ? (
                      <CheckCircle2 className="h-4 w-4 text-green-500" />
                    ) : (
                      <Loader2 className="h-4 w-4 animate-spin text-blue-500" />
                    )}
                    <span
                      className={
                        s.done ? 'text-muted-foreground' : 'text-foreground'
                      }
                    >
                      {s.message}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {activationStatus === 'success' && (
          <Result
            status="success"
            title="HA Setup Complete!"
            subTitle="All services are running successfully"
            extra={
              <div className="max-w-md mx-auto">
                <Button onClick={onDone}>Go to Dashboard</Button>
              </div>
            }
          />
        )}

        {activationStatus === 'error' && (
          <Result
            status="error"
            title="Activation Failed"
            subTitle={activationError ?? undefined}
            extra={
              <div className="flex items-center gap-3">
                <Button onClick={onRetry}>Retry Activation</Button>
                <Button variant="outline" onClick={onDone}>
                  Go to Dashboard
                </Button>
              </div>
            }
          />
        )}
      </CardContent>
    </Card>
  );
}
