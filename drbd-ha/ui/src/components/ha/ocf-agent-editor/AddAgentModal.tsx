import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { ResourceAgentsByProvider } from '@/api/ha-profiles';

interface AddAgentModalProps {
  visible: boolean;
  onOk: () => void;
  onCancel: () => void;
  agentType: 'ocf' | 'systemd';
  onAgentTypeChange: (type: 'ocf' | 'systemd') => void;
  systemdUnit: string;
  onSystemdUnitChange: (value: string) => void;
  selectedProvider: string;
  onProviderChange: (provider: string) => void;
  selectedAgent: string;
  onAgentChange: (agent: string) => void;
  allAgents: ResourceAgentsByProvider | null;
  currentTheme: string;
}

export function AddAgentModal({
  visible,
  onOk,
  onCancel,
  agentType,
  onAgentTypeChange,
  systemdUnit,
  onSystemdUnitChange,
  selectedProvider,
  onProviderChange,
  selectedAgent,
  onAgentChange,
  allAgents,
  currentTheme,
}: AddAgentModalProps) {
  return (
    <Dialog
      open={visible}
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <DialogContent className="max-w-[600px]">
        <DialogHeader>
          <DialogTitle>Add New Agent</DialogTitle>
        </DialogHeader>

        <div className="mt-4 flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label>Agent Type</Label>
            <Select
              value={agentType}
              onValueChange={(v) => onAgentTypeChange(v as 'ocf' | 'systemd')}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="ocf">OCF Agent</SelectItem>
                <SelectItem value="systemd">Systemd Unit</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {agentType === 'systemd' && (
            <div className="flex flex-col gap-1.5">
              <Label>Systemd Unit</Label>
              <Input
                placeholder="Enter systemd unit name"
                value={systemdUnit}
                onChange={(e) => onSystemdUnitChange(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') onOk();
                }}
              />
              <p className="text-xs text-muted-foreground">
                e.g., nginx.service, var-lib-mysql.mount
              </p>
            </div>
          )}

          {agentType === 'ocf' && (
            <>
              <div className="flex flex-col gap-1.5">
                <Label>Provider</Label>
                <Select
                  value={selectedProvider || undefined}
                  onValueChange={onProviderChange}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select provider" />
                  </SelectTrigger>
                  <SelectContent>
                    {allAgents
                      ? Object.keys(allAgents.providers)
                          .sort()
                          .map((p) => (
                            <SelectItem key={p} value={p}>
                              {p}
                            </SelectItem>
                          ))
                      : null}
                  </SelectContent>
                </Select>
              </div>

              <div className="flex flex-col gap-1.5">
                <Label>Agent</Label>
                <Select
                  value={selectedAgent || undefined}
                  onValueChange={onAgentChange}
                  disabled={!selectedProvider}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select agent" />
                  </SelectTrigger>
                  <SelectContent>
                    {selectedProvider && allAgents
                      ? allAgents.providers[selectedProvider]?.map((a) => (
                          <SelectItem key={a.name} value={a.name}>
                            {`${a.name} - ${a.shortdesc || ''}`}
                          </SelectItem>
                        ))
                      : null}
                  </SelectContent>
                </Select>
              </div>

              {selectedAgent && selectedProvider && allAgents && (
                <div className="flex flex-col gap-1.5">
                  <Label>Description</Label>
                  <div
                    className="rounded text-[13px]"
                    style={{
                      padding: '12px',
                      background:
                        currentTheme === 'dark' ? '#1e293b' : '#f8fafc',
                    }}
                  >
                    {allAgents.providers[selectedProvider]?.find(
                      (a) => a.name === selectedAgent,
                    )?.longdesc || 'No description available'}
                  </div>
                </div>
              )}
            </>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={onOk}>Add</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
