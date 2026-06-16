import { Info, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { nodesApi } from '@/api';
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
import { Spinner } from '@/components/ui/spinner';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { useNodesStore } from '@/stores/nodes';
import type { AddNodeRequest, Node } from '@/types';

const statusVariant: Record<
  string,
  'default' | 'secondary' | 'destructive' | 'outline'
> = {
  online: 'default',
  offline: 'destructive',
  error: 'secondary',
  unknown: 'outline',
};

const emptyForm: AddNodeRequest = {
  hostname: '',
  ip: '',
  ssh_port: 22,
  ssh_user: '',
} as AddNodeRequest;

export function Nodes() {
  const { nodes, loading, fetch, add, remove } = useNodesStore();
  const [modalOpen, setModalOpen] = useState(false);
  const [form, setForm] = useState<AddNodeRequest>(emptyForm);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    fetch();
  }, [fetch]);

  const resetForm = () => setForm(emptyForm);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.hostname?.trim() || !form.ip?.trim()) {
      toast.error('Hostname and IP Address are required');
      return;
    }
    setSubmitting(true);
    try {
      await add({
        ...form,
        ssh_user: form.ssh_user?.trim() || undefined,
      });
      toast.success('Node added successfully');
      setModalOpen(false);
      resetForm();
    } catch (err) {
      toast.error((err as { message: string }).message);
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!window.confirm('Delete this node?')) return;
    try {
      await remove(id);
      toast.success('Node removed');
    } catch (err) {
      toast.error((err as { message: string }).message);
    }
  };

  const handleCheck = async (id: string) => {
    try {
      const result = await nodesApi.check(id);
      if (result.status === 'online') {
        toast.success(`Node ${result.hostname} is online`);
      } else if (result.status === 'offline') {
        toast.error(
          `Node ${result.hostname}: ${result.message || 'SSH connection failed'}`,
        );
      } else {
        toast.error(
          `Node ${result.hostname}: ${result.message || result.status}`,
        );
      }
      fetch();
    } catch (err) {
      toast.error((err as { message: string }).message);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex justify-between items-center">
        <h2 className="text-2xl font-semibold">Nodes</h2>
        <Button onClick={() => setModalOpen(true)}>
          <Plus className="h-4 w-4" />
          Add Node
        </Button>
      </div>

      <div className="flex items-start gap-3 rounded-md border border-border bg-muted p-4 text-sm">
        <Info className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
        <div>
          <div className="font-medium">Remote access requirements</div>
          <div className="text-muted-foreground">
            Nodes must allow passwordless SSH. If SSH User is not root, that
            user must also support passwordless sudo (`sudo -n`). Node Check
            only reports online when these requirements pass.
          </div>
        </div>
      </div>

      <div className="rounded-md border border-border overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="bg-muted text-muted-foreground text-left">
              <th className="px-4 py-2 font-medium">Hostname</th>
              <th className="px-4 py-2 font-medium">IP</th>
              <th className="px-4 py-2 font-medium">SSH Port</th>
              <th className="px-4 py-2 font-medium">User</th>
              <th className="px-4 py-2 font-medium">Status</th>
              <th className="px-4 py-2 font-medium">Type</th>
              <th className="px-4 py-2 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={7} className="px-4 py-10 text-center">
                  <Spinner className="mx-auto h-5 w-5" />
                </td>
              </tr>
            ) : nodes.length === 0 ? (
              <tr>
                <td
                  colSpan={7}
                  className="px-4 py-10 text-center text-muted-foreground"
                >
                  No data
                </td>
              </tr>
            ) : (
              nodes.map((record: Node) => {
                const tag = (
                  <Badge variant={statusVariant[record.status] || 'outline'}>
                    {record.status.toUpperCase()}
                  </Badge>
                );
                return (
                  <tr key={record.id} className="border-t border-border">
                    <td className="px-4 py-2">{record.hostname}</td>
                    <td className="px-4 py-2">{record.ip}</td>
                    <td className="px-4 py-2">{record.ssh_port}</td>
                    <td className="px-4 py-2">{record.ssh_user}</td>
                    <td className="px-4 py-2">
                      {record.status_message ? (
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <span>{tag}</span>
                          </TooltipTrigger>
                          <TooltipContent>
                            {record.status_message}
                          </TooltipContent>
                        </Tooltip>
                      ) : (
                        tag
                      )}
                    </td>
                    <td className="px-4 py-2">
                      <Badge variant="secondary">
                        {record.is_local ? 'Local' : 'Remote'}
                      </Badge>
                    </td>
                    <td className="px-4 py-2">
                      <div className="flex gap-2">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleCheck(record.id)}
                        >
                          <RefreshCw className="h-4 w-4" />
                          Check
                        </Button>
                        {!record.is_local && (
                          <Button
                            variant="destructive"
                            size="sm"
                            onClick={() => handleDelete(record.id)}
                          >
                            <Trash2 className="h-4 w-4" />
                            Delete
                          </Button>
                        )}
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
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Add Node</DialogTitle>
          </DialogHeader>
          <form onSubmit={handleAdd} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="node-hostname">Hostname</Label>
              <Input
                id="node-hostname"
                placeholder="node2"
                value={form.hostname ?? ''}
                onChange={(e) =>
                  setForm((f) => ({ ...f, hostname: e.target.value }))
                }
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="node-ip">IP Address</Label>
              <Input
                id="node-ip"
                placeholder="192.168.1.102"
                value={form.ip ?? ''}
                onChange={(e) => setForm((f) => ({ ...f, ip: e.target.value }))}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="node-ssh-port">SSH Port</Label>
              <Input
                id="node-ssh-port"
                type="number"
                min={1}
                max={65535}
                value={form.ssh_port ?? 22}
                onChange={(e) =>
                  setForm((f) => ({
                    ...f,
                    ssh_port: Number(e.target.value),
                  }))
                }
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="node-ssh-user">SSH User</Label>
              <Input
                id="node-ssh-user"
                placeholder="cluster-admin"
                value={form.ssh_user ?? ''}
                onChange={(e) =>
                  setForm((f) => ({ ...f, ssh_user: e.target.value }))
                }
              />
              <p className="text-xs text-muted-foreground">
                Optional. Leave empty to use the global default SSH user. If it
                is not root, it must support passwordless sudo (`sudo -n`).
              </p>
            </div>
            <Button type="submit" disabled={submitting} className="w-full">
              {submitting && <Spinner className="mr-2 h-4 w-4" />}
              Add Node
            </Button>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  );
}
