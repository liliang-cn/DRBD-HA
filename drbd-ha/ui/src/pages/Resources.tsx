import { Plus, RefreshCw, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { nodesApi, resourcesApi } from '@/api';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Spinner } from '@/components/ui/spinner';
import { useNodesStore } from '@/stores/nodes';
import { useResourcesStore } from '@/stores/resources';
import type { BlockDevice, CreateResourceRequest, DrbdResource } from '@/types';

const roleVariant: Record<
  string,
  'default' | 'secondary' | 'destructive' | 'outline'
> = {
  Primary: 'default',
  Secondary: 'secondary',
  Unknown: 'outline',
};

const diskStateVariant: Record<
  string,
  'default' | 'secondary' | 'destructive' | 'outline'
> = {
  UpToDate: 'default',
  Inconsistent: 'secondary',
  Diskless: 'destructive',
  DUnknown: 'outline',
};

interface ResourceFormState {
  name: string;
  port: number;
  minor: number;
  auto_promote: boolean;
  node_disks: Record<string, string>;
}

const initialForm: ResourceFormState = {
  name: '',
  port: 7789,
  minor: 0,
  auto_promote: false,
  node_disks: {},
};

export function Resources() {
  const { resources, loading, fetch } = useResourcesStore();
  const { nodes, fetch: fetchNodes } = useNodesStore();
  const [modalOpen, setModalOpen] = useState(false);
  const [form, setForm] = useState<ResourceFormState>(initialForm);
  const [submitting, setSubmitting] = useState(false);
  const [availableDisks, setAvailableDisks] = useState<
    Record<string, BlockDevice[]>
  >({});

  useEffect(() => {
    fetch();
    fetchNodes();
  }, [fetch, fetchNodes]);

  useEffect(() => {
    // Load available disks for each node - only fetch if not already loaded
    nodes.forEach(async (node) => {
      if (!availableDisks[node.id]) {
        try {
          const disks = await nodesApi.getAvailableDisks(node.id);
          setAvailableDisks((prev) => ({ ...prev, [node.id]: disks }));
        } catch {}
      }
    });
  }, [nodes, availableDisks]);

  const resetForm = () => setForm(initialForm);

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(form.name)) {
      toast.error('Enter a valid resource name');
      return;
    }
    const missingDisk = nodes.find((node) => !form.node_disks[node.id]);
    if (missingDisk) {
      toast.error(`Select a disk for ${missingDisk.hostname}`);
      return;
    }
    setSubmitting(true);
    try {
      await resourcesApi.create(form as unknown as CreateResourceRequest);
      toast.success('Resource created');
      setModalOpen(false);
      resetForm();
      fetch();
    } catch (err) {
      toast.error((err as { message: string }).message);
    } finally {
      setSubmitting(false);
    }
  };

  const handleAction = async (name: string, action: string, force = false) => {
    try {
      const result = await resourcesApi.action(name, {
        action: action as any,
        force,
      });
      if (result.success) {
        toast.success(`${action} completed`);
      } else {
        toast.error(result.message || `${action} failed`);
      }
      fetch();
    } catch (err) {
      toast.error((err as { message: string }).message);
    }
  };

  const handleDelete = async (name: string) => {
    if (!window.confirm('Delete this resource?')) return;
    try {
      await resourcesApi.delete(name);
      toast.success('Resource deleted');
      fetch();
    } catch (err) {
      toast.error((err as { message: string }).message);
    }
  };

  const handleInit = async (name: string) => {
    try {
      await resourcesApi.init(name);
      toast.success('Resource initialized');
      fetch();
    } catch (err) {
      toast.error((err as { message: string }).message);
    }
  };

  const refreshAvailableDisks = async () => {
    const disksMap: Record<string, BlockDevice[]> = {};
    for (const node of nodes) {
      try {
        const disks = await nodesApi.getAvailableDisks(node.id);
        disksMap[node.id] = disks;
      } catch {}
    }
    setAvailableDisks(disksMap);
    toast.success('Available disks refreshed');
  };

  const runResourceAction = (record: DrbdResource, value: string) => {
    switch (value) {
      case 'up':
        return handleAction(record.name, 'up');
      case 'down':
        return handleAction(record.name, 'down');
      case 'primary':
        return handleAction(record.name, 'primary');
      case 'primary-force':
        return handleAction(record.name, 'primary', true);
      case 'secondary':
        return handleAction(record.name, 'secondary');
      case 'init':
        return handleInit(record.name);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h2 className="text-2xl font-semibold">DRBD Resources</h2>
        <div className="flex gap-2">
          <Button variant="outline" onClick={refreshAvailableDisks}>
            <RefreshCw className="h-4 w-4" />
            Refresh Disks
          </Button>
          <Button onClick={() => setModalOpen(true)}>
            <Plus className="h-4 w-4" />
            Create Resource
          </Button>
        </div>
      </div>

      <div className="rounded-md border border-border overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="bg-muted text-muted-foreground text-left">
              <th className="px-4 py-2 font-medium">Name</th>
              <th className="px-4 py-2 font-medium">Role</th>
              <th className="px-4 py-2 font-medium">Disk State</th>
              <th className="px-4 py-2 font-medium">Connections</th>
              <th className="px-4 py-2 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={5} className="px-4 py-10 text-center">
                  <Spinner className="mx-auto h-5 w-5" />
                </td>
              </tr>
            ) : resources.length === 0 ? (
              <tr>
                <td
                  colSpan={5}
                  className="px-4 py-10 text-center text-muted-foreground"
                >
                  No data
                </td>
              </tr>
            ) : (
              resources.map((record: DrbdResource) => {
                const diskState = record.devices[0]?.disk_state || 'Unknown';
                return (
                  <tr key={record.name} className="border-t border-border">
                    <td className="px-4 py-2">{record.name}</td>
                    <td className="px-4 py-2">
                      <Badge variant={roleVariant[record.role] || 'outline'}>
                        {record.role}
                      </Badge>
                    </td>
                    <td className="px-4 py-2">
                      <Badge
                        variant={diskStateVariant[diskState] || 'outline'}
                      >
                        {diskState}
                      </Badge>
                    </td>
                    <td className="px-4 py-2">
                      <div className="flex flex-wrap gap-2">
                        {record.connections.map((c) => (
                          <Badge
                            key={c.name}
                            variant={
                              c.connection_state === 'Connected'
                                ? 'default'
                                : 'secondary'
                            }
                          >
                            {c.name}: {c.connection_state}
                          </Badge>
                        ))}
                      </div>
                    </td>
                    <td className="px-4 py-2">
                      <div className="flex items-center gap-2">
                        <Select
                          value=""
                          onValueChange={(value) =>
                            runResourceAction(record, value)
                          }
                        >
                          <SelectTrigger className="h-8 w-32 text-xs">
                            <SelectValue placeholder="Actions" />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="up">Up</SelectItem>
                            <SelectItem value="down">Down</SelectItem>
                            <SelectSeparator />
                            <SelectItem value="primary">Primary</SelectItem>
                            <SelectItem value="primary-force">
                              Primary (Force)
                            </SelectItem>
                            <SelectItem value="secondary">
                              Secondary
                            </SelectItem>
                            <SelectSeparator />
                            <SelectItem value="init">Initialize</SelectItem>
                          </SelectContent>
                        </Select>
                        <Button
                          variant="destructive"
                          size="sm"
                          onClick={() => handleDelete(record.name)}
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      <Dialog
        open={modalOpen}
        onOpenChange={(open) => {
          setModalOpen(open);
          if (!open) resetForm();
        }}
      >
        <DialogContent className="max-w-[600px]">
          <DialogHeader>
            <DialogTitle>Create DRBD Resource</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleCreate} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="res-name">Resource Name</Label>
              <Input
                id="res-name"
                placeholder="r0"
                value={form.name}
                onChange={(e) =>
                  setForm((f) => ({ ...f, name: e.target.value }))
                }
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="res-port">DRBD Port</Label>
              <Input
                id="res-port"
                type="number"
                min={7000}
                max={8000}
                value={form.port}
                onChange={(e) =>
                  setForm((f) => ({ ...f, port: Number(e.target.value) }))
                }
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="res-minor">Minor Number</Label>
              <Input
                id="res-minor"
                type="number"
                min={0}
                value={form.minor}
                onChange={(e) =>
                  setForm((f) => ({ ...f, minor: Number(e.target.value) }))
                }
              />
            </div>
            <div className="space-y-2">
              <Label>Auto Promote</Label>
              <Select
                value={form.auto_promote ? 'true' : 'false'}
                onValueChange={(value) =>
                  setForm((f) => ({ ...f, auto_promote: value === 'true' }))
                }
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="true">Yes (Standard DRBD)</SelectItem>
                  <SelectItem value="false">
                    No (For HA/drbd-reactor)
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-3">
              <Label>Node Disks</Label>
              {nodes.map((node) => (
                <div key={node.id} className="space-y-2">
                  <Label className="text-xs text-muted-foreground">
                    {node.hostname} ({node.ip})
                  </Label>
                  <Select
                    value={form.node_disks[node.id] ?? ''}
                    onValueChange={(value) =>
                      setForm((f) => ({
                        ...f,
                        node_disks: { ...f.node_disks, [node.id]: value },
                      }))
                    }
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select disk" />
                    </SelectTrigger>
                    <SelectContent>
                      {(availableDisks[node.id] || []).map((d) => (
                        <SelectItem key={d.path} value={d.path}>
                          {d.path} ({d.size_human})
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              ))}
            </div>
            <Button type="submit" disabled={submitting} className="w-full">
              {submitting && <Spinner className="mr-2 h-4 w-4" />}
              Create Resource
            </Button>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
