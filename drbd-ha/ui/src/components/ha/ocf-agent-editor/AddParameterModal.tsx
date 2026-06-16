import type { OcfAgentWithMetadata } from '@/api/ha-profiles';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';

interface AddParameterModalProps {
  visible: boolean;
  onOk: () => void;
  onCancel: () => void;
  currentAgentIndex: number | null;
  selectedParam: string;
  onParamChange: (param: string) => void;
  parsedAgents: OcfAgentWithMetadata[];
}

export function AddParameterModal({
  visible,
  onOk,
  onCancel,
  currentAgentIndex,
  selectedParam,
  onParamChange,
  parsedAgents,
}: AddParameterModalProps) {
  // currentAgentIndex is an instanceId; resolve the matching agent.
  const agent =
    currentAgentIndex !== null
      ? (parsedAgents.find((a) => a.instanceId === currentAgentIndex) ??
        parsedAgents[currentAgentIndex])
      : null;

  const options =
    agent && agent.metadata
      ? (() => {
          const existingParams = new Set(
            (agent.item.ocf_agent?.params || []).map((p) => p.key),
          );
          return agent.metadata.parameters
            .filter((p) => !existingParams.has(p.name))
            .map((p) => ({
              label: `${p.name}${p.required ? ' (required)' : ''} - ${p.shortdesc || p.type}`,
              value: p.name,
            }));
        })()
      : [];

  const selectedMeta =
    selectedParam && agent?.metadata
      ? agent.metadata.parameters.find((p) => p.name === selectedParam)
      : undefined;

  return (
    <Dialog
      open={visible}
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <DialogContent className="max-w-[600px]">
        <DialogHeader>
          <DialogTitle>Add Parameter</DialogTitle>
        </DialogHeader>

        <div className="mt-4 flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label>Parameter</Label>
            <Select
              value={selectedParam || undefined}
              onValueChange={onParamChange}
            >
              <SelectTrigger>
                <SelectValue placeholder="Select parameter to add" />
              </SelectTrigger>
              <SelectContent>
                {options.map((o) => (
                  <SelectItem key={o.value} value={o.value}>
                    {o.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          {selectedParam && agent && (
            <>
              <div className="flex flex-col gap-1.5">
                <Label>Type</Label>
                <div>
                  <Badge variant="secondary">
                    {selectedMeta?.type || 'unknown'}
                  </Badge>
                </div>
              </div>

              <div className="flex flex-col gap-1.5">
                <Label>Description</Label>
                <div className="rounded border border-border bg-card p-3 text-[13px]">
                  {selectedMeta?.longdesc || 'No description available'}
                </div>
              </div>

              <div className="flex flex-col gap-1.5">
                <Label>Default Value</Label>
                <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-sm">
                  {selectedMeta?.default || '(empty)'}
                </code>
              </div>
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
