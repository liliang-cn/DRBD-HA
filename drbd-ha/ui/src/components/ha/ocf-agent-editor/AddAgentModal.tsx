import { Search } from 'lucide-react';
import { useMemo, useState } from 'react';
import { Badge } from '@/components/ui/badge';
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

interface FlatAgent {
  provider: string;
  name: string;
  shortdesc: string;
  longdesc: string;
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
  const [searchText, setSearchText] = useState('');

  // Flatten all providers into one searchable list
  const flatAgents = useMemo<FlatAgent[]>(() => {
    if (!allAgents) return [];
    return Object.entries(allAgents.providers)
      .sort(([a], [b]) => a.localeCompare(b))
      .flatMap(([provider, agents]) =>
        agents.map((a) => ({
          provider,
          name: a.name,
          shortdesc: a.shortdesc || '',
          longdesc: a.longdesc || '',
        })),
      );
  }, [allAgents]);

  // Filter by provider, agent name or description (case-insensitive)
  const filteredAgents = useMemo(() => {
    const q = searchText.trim().toLowerCase();
    if (!q) return flatAgents;
    return flatAgents.filter(
      (a) =>
        a.provider.toLowerCase().includes(q) ||
        a.name.toLowerCase().includes(q) ||
        a.shortdesc.toLowerCase().includes(q),
    );
  }, [flatAgents, searchText]);

  const selected = flatAgents.find(
    (a) => a.provider === selectedProvider && a.name === selectedAgent,
  );

  return (
    <Dialog
      open={visible}
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <DialogContent className="max-w-[640px]">
        <DialogHeader>
          <DialogTitle>Add New Agent</DialogTitle>
        </DialogHeader>

        <div className="mt-2 flex flex-col gap-4">
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
              <div className="relative">
                <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  className="pl-8"
                  placeholder="Search agents by provider, name or description..."
                  value={searchText}
                  onChange={(e) => setSearchText(e.target.value)}
                />
              </div>

              <div className="max-h-[260px] overflow-y-auto rounded-md border border-border">
                {filteredAgents.length === 0 ? (
                  <div className="p-4 text-center text-sm text-muted-foreground">
                    {allAgents ? 'No agents match the search' : 'Loading agents...'}
                  </div>
                ) : (
                  filteredAgents.map((a) => {
                    const isSelected =
                      a.provider === selectedProvider && a.name === selectedAgent;
                    return (
                      <button
                        type="button"
                        key={`${a.provider}:${a.name}`}
                        onClick={() => {
                          onProviderChange(a.provider);
                          onAgentChange(a.name);
                        }}
                        className={`flex w-full items-center gap-2 border-b border-border px-3 py-2 text-left text-sm last:border-b-0 hover:bg-accent ${
                          isSelected ? 'bg-accent' : ''
                        }`}
                      >
                        <Badge variant="outline" className="shrink-0">
                          {a.provider}
                        </Badge>
                        <span className="shrink-0 font-medium">{a.name}</span>
                        <span className="truncate text-xs text-muted-foreground">
                          {a.shortdesc}
                        </span>
                      </button>
                    );
                  })
                )}
              </div>

              {selected && (
                <div className="flex flex-col gap-1.5">
                  <Label>Description</Label>
                  <div
                    className="max-h-[120px] overflow-y-auto rounded text-[13px]"
                    style={{
                      padding: '12px',
                      background:
                        currentTheme === 'dark' ? '#1e293b' : '#f8fafc',
                    }}
                  >
                    {selected.longdesc || 'No description available'}
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
          <Button
            onClick={onOk}
            disabled={agentType === 'ocf' ? !selected : !systemdUnit.trim()}
          >
            Add
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
