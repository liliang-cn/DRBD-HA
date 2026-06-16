import { Info, LayoutGrid } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import {
  type AgentSummary,
  type ResourceAgent,
  resourceAgentsApi,
} from '@/api/resource-agents';
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
import { Spinner } from '@/components/ui/spinner';
import { Stepper } from '@/components/ui/stepper';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import type { OcfAgentConfig } from '@/types';

interface OcfAgentModalProps {
  visible: boolean;
  onCancel: () => void;
  onAdd: (config: OcfAgentConfig) => void;
}

const PAGE_SIZE = 10;

export function OcfAgentModal({
  visible,
  onCancel,
  onAdd,
}: OcfAgentModalProps) {
  const [step, setStep] = useState(0);
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedAgentSummary, setSelectedAgentSummary] =
    useState<AgentSummary | null>(null);
  const [metadata, setMetadata] = useState<ResourceAgent | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [searchText, setSearchText] = useState('');
  const [selectedProvider, setSelectedProvider] = useState<string>('all');
  const [page, setPage] = useState(0);

  const loadAgents = async () => {
    setLoading(true);
    try {
      const list = await resourceAgentsApi.list();
      setAgents(list);
    } catch (_err) {
      toast.error('Failed to load resource agents');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (visible && step === 0) {
      loadAgents();
    }
  }, [visible, step, loadAgents]);

  const providers = useMemo(() => {
    const unique = new Set(agents.map((a) => a.provider));
    return ['all', ...Array.from(unique).sort()];
  }, [agents]);

  const filteredAgents = useMemo(() => {
    return agents.filter((a) => {
      const matchesProvider =
        selectedProvider === 'all' || a.provider === selectedProvider;
      const matchesSearch =
        searchText === '' ||
        a.name.toLowerCase().includes(searchText.toLowerCase()) ||
        a.provider.toLowerCase().includes(searchText.toLowerCase());
      return matchesProvider && matchesSearch;
    });
  }, [agents, selectedProvider, searchText]);

  const pagedAgents = useMemo(
    () => filteredAgents.slice(page * PAGE_SIZE, page * PAGE_SIZE + PAGE_SIZE),
    [filteredAgents, page],
  );
  const totalPages = Math.max(1, Math.ceil(filteredAgents.length / PAGE_SIZE));

  // Reset page when filters change
  useEffect(() => {
    setPage(0);
  }, [selectedProvider, searchText]);

  const setValue = (name: string, value: string) => {
    setValues((prev) => ({ ...prev, [name]: value }));
  };

  const handleAgentSelect = async (agent: AgentSummary) => {
    setSelectedAgentSummary(agent);
    setLoading(true);
    try {
      const meta = await resourceAgentsApi.getMetadata(
        agent.provider,
        agent.name,
      );
      setMetadata(meta);
      setStep(1);
      // Set default values
      const defaultValues: Record<string, string> = {};
      meta.parameters.parameter.forEach((p) => {
        if (p.content['@default']) {
          defaultValues[p['@name']] = p.content['@default'];
        }
      });
      // Also set instance name default
      defaultValues.instance_name = `${agent.name}_1`;
      setValues(defaultValues);
    } catch (_err) {
      toast.error('Failed to load agent metadata');
    } finally {
      setLoading(false);
    }
  };

  const handleFinish = () => {
    const instanceName = values.instance_name;
    if (!instanceName || instanceName.trim() === '') {
      toast.error('Instance name is required');
      return;
    }
    // Validate required params
    if (metadata) {
      for (const param of metadata.parameters.parameter) {
        if (
          param['@required'] === '1' &&
          (!values[param['@name']] || values[param['@name']] === '')
        ) {
          toast.error(`${param['@name']} is required`);
          return;
        }
      }
    }

    const params: Record<string, string> = {};
    // Filter out instance_name from params and ignore empty optional params
    Object.entries(values).forEach(([key, value]) => {
      if (key !== 'instance_name' && value !== undefined && value !== '') {
        params[key] = String(value);
      }
    });

    if (selectedAgentSummary) {
      onAdd({
        name: `ocf:${selectedAgentSummary.provider}:${selectedAgentSummary.name}`,
        instance_name: instanceName,
        params,
      });
      reset();
    }
  };

  const reset = () => {
    setStep(0);
    setSelectedAgentSummary(null);
    setMetadata(null);
    setValues({});
    setSearchText('');
    setSelectedProvider('all');
    setPage(0);
  };

  const handleCancel = () => {
    reset();
    onCancel();
  };

  const renderAgentList = () => (
    <div className="flex h-[400px] rounded-md border border-border bg-background">
      <div className="w-44 shrink-0 overflow-auto border-r border-border p-2">
        {providers.map((p) => (
          <button
            key={p}
            type="button"
            onClick={() => setSelectedProvider(p)}
            className={cn(
              'flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm',
              selectedProvider === p
                ? 'bg-primary/10 text-primary font-medium'
                : 'text-foreground hover:bg-muted',
            )}
          >
            <LayoutGrid className="h-4 w-4" />
            {p === 'all' ? 'All Providers' : p}
          </button>
        ))}
      </div>
      <div className="flex flex-1 flex-col p-4">
        <div className="mb-4">
          <Input
            placeholder="Search agents..."
            value={searchText}
            onChange={(e) => setSearchText(e.target.value)}
          />
        </div>
        <div className="flex-1 overflow-auto">
          {loading ? (
            <div className="flex h-full items-center justify-center">
              <Spinner size={24} />
            </div>
          ) : (
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-muted text-muted-foreground text-left">
                  <th className="px-3 py-2 font-medium w-24">Provider</th>
                  <th className="px-3 py-2 font-medium">Name</th>
                  <th className="px-3 py-2 font-medium w-20">Action</th>
                </tr>
              </thead>
              <tbody>
                {pagedAgents.map((record) => (
                  <tr
                    key={`${record.provider}:${record.name}`}
                    className="border-t border-border"
                  >
                    <td className="px-3 py-2">{record.provider}</td>
                    <td className="px-3 py-2">{record.name}</td>
                    <td className="px-3 py-2">
                      <Button
                        variant="link"
                        size="sm"
                        onClick={() => handleAgentSelect(record)}
                      >
                        Select
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
        {!loading && totalPages > 1 && (
          <div className="mt-2 flex items-center justify-end gap-2 text-sm">
            <Button
              variant="outline"
              size="sm"
              disabled={page === 0}
              onClick={() => setPage((p) => Math.max(0, p - 1))}
            >
              Prev
            </Button>
            <span className="text-muted-foreground">
              {page + 1} / {totalPages}
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={page >= totalPages - 1}
              onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
            >
              Next
            </Button>
          </div>
        )}
      </div>
    </div>
  );

  const renderConfigForm = () => {
    if (!metadata) return null;

    return (
      <div className="max-h-[400px] space-y-4 overflow-auto pr-2">
        <div className="mb-4 rounded bg-muted p-3">
          <span className="font-semibold">{metadata.name}</span>
          <div className="text-sm text-muted-foreground">
            {metadata.shortdesc.text}
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            {metadata.longdesc.text}
          </div>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="instance_name">Instance Name</Label>
          <Input
            id="instance_name"
            value={values.instance_name ?? ''}
            onChange={(e) => setValue('instance_name', e.target.value)}
          />
          <p className="text-xs text-muted-foreground">
            Unique identifier for this agent instance
          </p>
        </div>

        <div className="flex items-center gap-2">
          <hr className="flex-1 border-border" />
          <span className="text-sm text-muted-foreground">Parameters</span>
          <hr className="flex-1 border-border" />
        </div>

        {metadata.parameters.parameter.map((param) => {
          const name = param['@name'];
          const type = param.content['@type'];
          return (
            <div key={name} className="space-y-1.5">
              <Label className="flex items-center gap-2">
                {name}
                {param['@required'] === '1' && (
                  <span className="text-destructive">*</span>
                )}
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span className="inline-flex cursor-help text-muted-foreground">
                      <Info className="h-3.5 w-3.5" />
                    </span>
                  </TooltipTrigger>
                  <TooltipContent className="max-w-xs">
                    {param.longdesc.text}
                  </TooltipContent>
                </Tooltip>
              </Label>
              {type === 'boolean' ? (
                <Select
                  value={values[name] ?? ''}
                  onValueChange={(v) => setValue(name, v)}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="true">true</SelectItem>
                    <SelectItem value="false">false</SelectItem>
                  </SelectContent>
                </Select>
              ) : type === 'integer' ? (
                <Input
                  type="number"
                  value={values[name] ?? ''}
                  onChange={(e) => setValue(name, e.target.value)}
                />
              ) : (
                <Input
                  value={values[name] ?? ''}
                  onChange={(e) => setValue(name, e.target.value)}
                />
              )}
              <p className="text-xs text-muted-foreground">
                {param.shortdesc.text}
              </p>
            </div>
          );
        })}
      </div>
    );
  };

  return (
    <Dialog
      open={visible}
      onOpenChange={(open) => {
        if (!open) handleCancel();
      }}
    >
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>Add OCF Resource Agent</DialogTitle>
        </DialogHeader>

        <Stepper
          className="mb-4"
          current={step}
          steps={[{ title: 'Select Agent' }, { title: 'Configure' }]}
        />

        {step === 0 ? renderAgentList() : renderConfigForm()}

        {step === 1 && (
          <DialogFooter className="justify-between sm:justify-between">
            <Button variant="outline" onClick={() => setStep(0)}>
              Back
            </Button>
            <Button onClick={handleFinish}>Add Agent</Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  );
}
