import { Info, Pencil, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { nodesApi } from '@/api';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
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
import type { WizardSharedState } from './types';

const statusVariant: Record<
  string,
  'default' | 'secondary' | 'destructive' | 'outline'
> = {
  online: 'default',
  offline: 'destructive',
  error: 'secondary',
  unknown: 'outline',
};

interface NodeFormValues {
  hostname: string;
  ip: string;
  ssh_port: number;
  ssh_user: string;
}

const emptyForm: NodeFormValues = {
  hostname: '',
  ip: '',
  ssh_port: 22,
  ssh_user: '',
};

interface NodesVerificationStepProps {
  nodes: Node[];
  sharedState: WizardSharedState;
}

export function NodesVerificationStep({
  nodes,
  sharedState,
}: NodesVerificationStepProps) {
  const { add, remove, fetch, update } = useNodesStore();
  const [modalOpen, setModalOpen] = useState(false);
  const [editModalOpen, setEditModalOpen] = useState(false);
  const [editingNode, setEditingNode] = useState<Node | null>(null);
  const [form, setForm] = useState<NodeFormValues>(emptyForm);
  const [submitting, setSubmitting] = useState(false);
  const [selectedRowKeys, setSelectedRowKeys] = useState<string[]>([]);

  // Initialize selected nodes with all nodes by default
  useEffect(() => {
    if (nodes.length > 0 && selectedRowKeys.length === 0) {
      const allKeys = nodes.map((n) => n.id);
      setSelectedRowKeys(allKeys);
      sharedState.setSelectedNodes(nodes);
    }
  }, [nodes, selectedRowKeys.length, sharedState.setSelectedNodes]);

  const toggleSelection = (node: Node, checked: boolean) => {
    const newKeys = checked
      ? [...selectedRowKeys, node.id]
      : selectedRowKeys.filter((k) => k !== node.id);
    setSelectedRowKeys(newKeys);
    const selectedNodes = nodes.filter((n) => newKeys.includes(n.id));
    sharedState.setSelectedNodes(selectedNodes);
  };

  const setField = (key: keyof NodeFormValues, value: string | number) => {
    setForm((prev) => ({ ...prev, [key]: value }));
  };

  const handleAdd = async () => {
    if (!form.hostname.trim()) {
      toast.error('Hostname is required');
      return;
    }
    if (!form.ip.trim()) {
      toast.error('IP Address is required');
      return;
    }
    setSubmitting(true);
    try {
      const payload: AddNodeRequest = {
        hostname: form.hostname,
        ip: form.ip,
        ssh_port: form.ssh_port,
        ssh_user: form.ssh_user?.trim() || undefined,
      };
      await add(payload);
      toast.success('Node added successfully');
      setModalOpen(false);
      setForm(emptyForm);
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

  const handleEdit = (node: Node) => {
    setEditingNode(node);
    setForm({
      hostname: node.hostname,
      ip: node.ip,
      ssh_port: node.ssh_port,
      ssh_user: node.ssh_user || '',
    });
    setEditModalOpen(true);
  };

  const handleUpdate = async () => {
    if (!editingNode) return;
    if (!form.ip.trim()) {
      toast.error('IP Address is required');
      return;
    }
    setSubmitting(true);
    try {
      await update(editingNode.id, {
        hostname: form.hostname,
        ip: form.ip,
        ssh_port: form.ssh_port,
        ssh_user: form.ssh_user?.trim() || undefined,
      });
      toast.success('Node updated successfully');
      setEditModalOpen(false);
      setEditingNode(null);
      setForm(emptyForm);
      fetch();
    } catch (err) {
      toast.error((err as { message: string }).message);
    } finally {
      setSubmitting(false);
    }
  };

  const infoAlert = (title: string, description: React.ReactNode) => (
    <div className="flex gap-3 rounded-lg border border-blue-500/30 bg-blue-500/10 p-3 text-sm">
      <Info className="mt-0.5 h-4 w-4 shrink-0 text-blue-500" />
      <div>
        <div className="font-medium text-foreground">{title}</div>
        <div className="mt-0.5 text-muted-foreground">{description}</div>
      </div>
    </div>
  );

  return (
    <Card className="w-full">
      <CardHeader>
        <div className="flex justify-between items-center">
          <CardTitle className="text-lg font-semibold">
            Step 1: Select or Add Cluster Nodes
          </CardTitle>
          <Button onClick={() => setModalOpen(true)}>
            <Plus className="mr-2 h-4 w-4" />
            Add Node
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <div className="p-4">
          {infoAlert(
            'Remote access requirements',
            'Each selected node must allow passwordless SSH. If the SSH user is not root, it must also allow passwordless sudo (`sudo -n`). The Check action only reports online when these validations pass.',
          )}
          <div className="mb-4" />

          {nodes.length === 0 ? (
            infoAlert(
              'No nodes available',
              <div>
                <p>Please add at least 2 nodes to configure HA.</p>
                <Button className="mt-2" onClick={() => setModalOpen(true)}>
                  <Plus className="mr-2 h-4 w-4" />
                  Add Node
                </Button>
              </div>,
            )
          ) : (
            <>
              <div className="overflow-x-auto rounded-lg border border-border">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="bg-muted text-muted-foreground text-left">
                      <th className="px-4 py-2 font-medium w-10"></th>
                      <th className="px-4 py-2 font-medium">Hostname</th>
                      <th className="px-4 py-2 font-medium">IP</th>
                      <th className="px-4 py-2 font-medium">SSH User</th>
                      <th className="px-4 py-2 font-medium">Status</th>
                      <th className="px-4 py-2 font-medium">Type</th>
                      <th className="px-4 py-2 font-medium">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {nodes.map((record) => {
                      const checkboxDisabled = record.status !== 'online';
                      const statusTag = (
                        <Badge variant={statusVariant[record.status]}>
                          {record.status.toUpperCase()}
                        </Badge>
                      );
                      return (
                        <tr key={record.id} className="border-t border-border">
                          <td className="px-4 py-2">
                            <input
                              type="checkbox"
                              checked={selectedRowKeys.includes(record.id)}
                              disabled={checkboxDisabled}
                              onChange={(e) =>
                                toggleSelection(record, e.target.checked)
                              }
                            />
                          </td>
                          <td className="px-4 py-2">{record.hostname}</td>
                          <td className="px-4 py-2">{record.ip}</td>
                          <td className="px-4 py-2">
                            <div className="flex items-center gap-2">
                              {record.ssh_user ? (
                                <span>{record.ssh_user}</span>
                              ) : (
                                <Badge variant="destructive">Not set</Badge>
                              )}
                              {!record.ssh_user && (
                                <Button
                                  variant="link"
                                  size="sm"
                                  onClick={() => handleEdit(record)}
                                >
                                  <Pencil className="mr-1 h-3 w-3" />
                                  Set
                                </Button>
                              )}
                            </div>
                          </td>
                          <td className="px-4 py-2">
                            {record.status_message ? (
                              <Tooltip>
                                <TooltipTrigger asChild>
                                  <span>{statusTag}</span>
                                </TooltipTrigger>
                                <TooltipContent>
                                  {record.status_message}
                                </TooltipContent>
                              </Tooltip>
                            ) : (
                              statusTag
                            )}
                          </td>
                          <td className="px-4 py-2">
                            <Badge variant="outline">
                              {record.is_local ? 'Local' : 'Remote'}
                            </Badge>
                          </td>
                          <td className="px-4 py-2">
                            <div className="flex items-center gap-2">
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() => handleCheck(record.id)}
                              >
                                <RefreshCw className="mr-1 h-3 w-3" />
                                Check
                              </Button>
                              <Button
                                variant="outline"
                                size="sm"
                                onClick={() => handleEdit(record)}
                              >
                                <Pencil className="mr-1 h-3 w-3" />
                                Edit
                              </Button>
                              {!record.is_local && (
                                <Button
                                  variant="destructive"
                                  size="sm"
                                  onClick={() => handleDelete(record.id)}
                                >
                                  <Trash2 className="mr-1 h-3 w-3" />
                                  Delete
                                </Button>
                              )}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>

              {selectedRowKeys.length < 2 && (
                <div className="mt-6 flex gap-3 rounded-lg border border-yellow-500/30 bg-yellow-500/10 p-3 text-sm">
                  <Info className="mt-0.5 h-4 w-4 shrink-0 text-yellow-500" />
                  <span className="text-foreground">
                    At least 2 nodes must be selected for HA
                  </span>
                </div>
              )}
            </>
          )}
        </div>
      </CardContent>

      {/* Add Node Dialog */}
      <Dialog
        open={modalOpen}
        onOpenChange={(open) => {
          setModalOpen(open);
          if (!open) setForm(emptyForm);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Add Cluster Node</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="add-hostname">Hostname</Label>
              <Input
                id="add-hostname"
                placeholder="node2"
                value={form.hostname}
                onChange={(e) => setField('hostname', e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="add-ip">IP Address</Label>
              <Input
                id="add-ip"
                placeholder="192.168.1.102"
                value={form.ip}
                onChange={(e) => setField('ip', e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="add-port">SSH Port</Label>
              <Input
                id="add-port"
                type="number"
                min={1}
                max={65535}
                value={form.ssh_port}
                onChange={(e) => setField('ssh_port', Number(e.target.value))}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="add-user">SSH User</Label>
              <Input
                id="add-user"
                placeholder="cluster-admin"
                value={form.ssh_user}
                onChange={(e) => setField('ssh_user', e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                Optional. Leave empty to use the global default SSH user. If it
                is not root, it must support passwordless sudo (`sudo -n`).
              </p>
            </div>
            <Button
              className="w-full"
              disabled={submitting}
              onClick={handleAdd}
            >
              {submitting && <Spinner className="mr-2 h-4 w-4" />}
              Add Node
            </Button>
          </div>
        </DialogContent>
      </Dialog>

      {/* Edit Node Dialog */}
      <Dialog
        open={editModalOpen}
        onOpenChange={(open) => {
          setEditModalOpen(open);
          if (!open) {
            setEditingNode(null);
            setForm(emptyForm);
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Edit Node</DialogTitle>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="edit-hostname">Hostname</Label>
              <Input id="edit-hostname" value={form.hostname} disabled />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="edit-ip">IP Address</Label>
              <Input
                id="edit-ip"
                value={form.ip}
                onChange={(e) => setField('ip', e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="edit-port">SSH Port</Label>
              <Input
                id="edit-port"
                type="number"
                min={1}
                max={65535}
                value={form.ssh_port}
                onChange={(e) => setField('ssh_port', Number(e.target.value))}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="edit-user">SSH User</Label>
              <Input
                id="edit-user"
                placeholder="cluster-admin"
                value={form.ssh_user}
                onChange={(e) => setField('ssh_user', e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                Optional. Leave empty to use the global default SSH user. If it
                is not root, it must support passwordless sudo (`sudo -n`).
              </p>
            </div>
            <Button
              className="w-full"
              disabled={submitting}
              onClick={handleUpdate}
            >
              {submitting && <Spinner className="mr-2 h-4 w-4" />}
              Update Node
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
