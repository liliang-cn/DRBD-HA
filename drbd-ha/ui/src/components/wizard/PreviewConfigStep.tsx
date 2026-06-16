import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';

interface PreviewConfigStepProps {
  configContent: string | null;
  drbdConfigContent?: string | null;
}

export function PreviewConfigStep({
  configContent,
  drbdConfigContent,
}: PreviewConfigStepProps) {
  return (
    <Card className="w-full">
      <CardHeader>
        <CardTitle>Configuration Preview</CardTitle>
      </CardHeader>
      <CardContent>
        <p className="mb-4 text-sm text-foreground">
          Review the generated configurations before activating the HA profile.
        </p>
        <Tabs defaultValue="drbd">
          <TabsList>
            <TabsTrigger value="drbd">
              <strong>DRBD</strong>&nbsp;Configuration
            </TabsTrigger>
            <TabsTrigger value="reactor">
              <strong>drbd-reactor</strong>&nbsp;Configuration
            </TabsTrigger>
          </TabsList>
          <TabsContent value="drbd">
            <div className="space-y-4">
              <p className="text-sm text-foreground">
                Below is the generated <code>DRBD</code> resource configuration
                file.
              </p>
              <textarea
                value={drbdConfigContent || 'No configuration generated yet.'}
                readOnly
                rows={20}
                className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm font-mono shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
              <p className="mt-4 text-sm text-muted-foreground">
                This file is deployed to <code>/etc/drbd.d/</code> on all
                cluster nodes.
              </p>
            </div>
          </TabsContent>
          <TabsContent value="reactor">
            <div className="space-y-4">
              <p className="text-sm text-foreground">
                Below is the generated <code>drbd-reactor</code> promoter
                configuration file.
              </p>
              <textarea
                value={configContent || 'No configuration generated yet.'}
                readOnly
                rows={20}
                className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm font-mono shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              />
              <p className="mt-4 text-sm text-muted-foreground">
                This file will be deployed to{' '}
                <code>/etc/drbd-reactor.d/{configContent ? '*.toml' : ''}</code>{' '}
                on all cluster nodes.
              </p>
            </div>
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>
  );
}
