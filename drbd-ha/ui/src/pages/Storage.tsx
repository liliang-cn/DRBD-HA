import { HardDrive, Plus, RefreshCw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { nodesApi, storageApi } from '@/api';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
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
import type { BlockDevice, Node, StoragePool } from '@/types';

export function Storage() {
  const [pools, setPools] = useState<StoragePool[]>([]);
  const [loading, setLoading] = useState(false);
  const [modalVisible, setModalVisible] = useState(false);
  const [createLoading, setCreateLoading] = useState(false);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [disksByNode, setDisksByNode] = useState<Record<string, BlockDevice[]>>(
    {},
  );
  const [selectedNodes, setSelectedNodes] = useState<string[]>([]);
  const [poolName, setPoolName] = useState('');
  const [deviceByNode, setDeviceByNode] = useState<Record<string, string>>({});

  const fetchPools = async () => {
    setLoading(true);
    try {
      const { pools } = await storageApi.listPools();
      setPools(pools);
    } catch (error) {
      console.error(error);
      toast.error('Failed to load storage pools');
    } finally {
      setLoading(false);
    }
  };

  const fetchNodes = async () => {
    try {
      const data = await nodesApi.list();
      setNodes(data);

      // Fetch disks for new nodes only (avoid excessive API calls)
      const disksMap: Record<string, BlockDevice[]> = { ...disksByNode };
      for (const node of data) {
        if (!disksMap[node.id]) {
          try {
            disksMap[node.id] = await nodesApi.getAvailableDisks(node.id);
          } catch (error) {
            console.error(`Failed to fetch disks for node ${node.id}:`, error);
            disksMap[node.id] = [];
          }
        }
      }
      setDisksByNode(disksMap);
    } catch (error) {
      console.error(error);
    }
  };

  const refreshAvailableDisks = async (nodeId?: string) => {
    if (nodeId) {
      // Refresh for specific node
      try {
        const disks = await nodesApi.getAvailableDisks(nodeId);
        setDisksByNode((prev) => ({ ...prev, [nodeId]: disks }));
        toast.success(`Available disks refreshed for node ${nodeId}`);
      } catch (error) {
        console.error(`Failed to refresh disks for node ${nodeId}:`, error);
        toast.error(`Failed to refresh disks for node ${nodeId}`);
      }
    } else {
      // Refresh for all nodes
      const disksMap: Record<string, BlockDevice[]> = {};
      for (const node of nodes) {
        try {
          disksMap[node.id] = await nodesApi.getAvailableDisks(node.id);
        } catch (error) {
          console.error(`Failed to fetch disks for node ${node.id}:`, error);
          disksMap[node.id] = [];
        }
      }
      setDisksByNode(disksMap);
      toast.success('Available disks refreshed for all nodes');
    }
  };

  useEffect(() => {
    fetchPools();
    fetchNodes();
  }, [fetchNodes, fetchPools]); // Only run on mount, and fetchNodes internally handles avoiding duplicates

  const resetForm = () => {
    setPoolName('');
    setDeviceByNode({});
    setSelectedNodes([]);
  };

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!poolName.trim()) {
      toast.error('Please enter pool name');
      return;
    }
    if (selectedNodes.length === 0) {
      toast.error('Please select at least one node');
      return;
    }

    const missingDevice = selectedNodes.find((id) => !deviceByNode[id]);
    if (missingDevice) {
      toast.error('Please select a device for each selected node');
      return;
    }

    setCreateLoading(true);
    try {
      const nodeDevices: Record<string, string> = {};
      selectedNodes.forEach((nodeId) => {
        const device = deviceByNode[nodeId];
        if (device) {
          nodeDevices[nodeId] = device;
        }
      });

      await storageApi.createPool({
        name: poolName,
        pool_type: 'lvm',
        node_devices: nodeDevices,
      });
      toast.success('Storage pool created successfully on selected nodes');
      setModalVisible(false);
      resetForm();
      fetchPools();
    } catch (error) {
      const message =
        (error as { response?: { data?: { message?: string } } }).response?.data
          ?.message || 'Failed to create storage pool';
      toast.error(message);
    } finally {
      setCreateLoading(false);
    }
  };

  const nodeName = (nodeId: string) => {
    let node = nodes.find((n) => n.id === nodeId);
    if (!node && nodeId === 'local') {
      node = nodes.find((n) => n.is_local);
    }
    return node ? node.hostname : nodeId;
  };

  const sortedPools = [...pools].sort((a, b) => a.name.localeCompare(b.name));

  const totalCapacity =
    pools.reduce((acc, curr) => acc + curr.total_size, 0) / 1024 / 1024 / 1024;
  const availableFree =
    pools.reduce((acc, curr) => acc + curr.free_size, 0) / 1024 / 1024 / 1024;

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <h2 className="text-xl font-semibold">Storage Pools</h2>
        <div className="flex gap-2">
          <Button variant="outline" onClick={fetchPools} disabled={loading}>
            {loading ? (
              <Spinner className="h-4 w-4" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            Refresh Pools
          </Button>
          <Button variant="outline" onClick={() => refreshAvailableDisks()}>
            <RefreshCw className="h-4 w-4" />
            Refresh Disks
          </Button>
          <Button onClick={() => setModalVisible(true)}>
            <Plus className="h-4 w-4" />
            Create Pool
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
        <Card>
          <CardContent className="pt-6">
            <div className="text-sm text-muted-foreground">Total Pools</div>
            <div className="mt-1 flex items-center gap-2 text-2xl font-semibold">
              <HardDrive className="h-5 w-5" />
              {pools.length}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-6">
            <div className="text-sm text-muted-foreground">Total Capacity</div>
            <div className="mt-1 text-2xl font-semibold">
              {totalCapacity.toFixed(2)} GB
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="pt-6">
            <div className="text-sm text-muted-foreground">Available Free</div>
            <div
              className="mt-1 text-2xl font-semibold"
              style={{ color: '#3f8600' }}
            >
              {availableFree.toFixed(2)} GB
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="rounded-md border border-border overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="bg-muted text-muted-foreground text-left">
              <th className="px-4 py-2 font-medium">Name</th>
              <th className="px-4 py-2 font-medium">Node</th>
              <th className="px-4 py-2 font-medium">Device</th>
              <th className="px-4 py-2 font-medium">Total Size</th>
              <th className="px-4 py-2 font-medium">Free Size</th>
            </tr>
          </thead>
          <tbody>
            {loading ? (
              <tr>
                <td colSpan={5} className="px-4 py-10 text-center">
                  <Spinner className="mx-auto h-5 w-5" />
                </td>
              </tr>
            ) : sortedPools.length === 0 ? (
              <tr>
                <td
                  colSpan={5}
                  className="px-4 py-10 text-center text-muted-foreground"
                >
                  No data
                </td>
              </tr>
            ) : (
              sortedPools.map((pool) => (
                <tr key={pool.id} className="border-t border-border">
                  <td className="px-4 py-2">{pool.name}</td>
                  <td className="px-4 py-2">
                    {nodeName((pool as { node_id?: string }).node_id ?? '')}
                  </td>
                  <td className="px-4 py-2">{pool.device}</td>
                  <td className="px-4 py-2">
                    {(pool.total_size / 1024 / 1024 / 1024).toFixed(2)} GB
                  </td>
                  <td className="px-4 py-2">
                    {(pool.free_size / 1024 / 1024 / 1024).toFixed(2)} GB
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <Dialog
        open={modalVisible}
        onOpenChange={(open) => {
          setModalVisible(open);
          if (!open) resetForm();
        }}
      >
        <DialogContent className="max-w-[700px]">
          <DialogHeader>
            <DialogTitle>Create Storage Pool</DialogTitle>
          </DialogHeader>
          <form
            id="create-pool-form"
            onSubmit={handleCreate}
            className="space-y-4"
          >
            <div className="space-y-2">
              <Label htmlFor="pool-name">Pool Name</Label>
              <Input
                id="pool-name"
                placeholder="e.g., ha_pool"
                value={poolName}
                onChange={(e) => setPoolName(e.target.value)}
              />
            </div>

            <div className="space-y-2">
              <Label>Create Pool On Nodes</Label>
              <div className="rounded-md border border-border p-3">
                {nodes.length === 0 ? (
                  <p>No nodes available</p>
                ) : (
                  <div className="space-y-3">
                    {nodes.map((node) => {
                      const checked = selectedNodes.includes(node.id);
                      return (
                        <div
                          key={node.id}
                          className="border-b border-border pb-3 last:border-b-0 last:pb-0"
                        >
                          <label className="flex items-center gap-2">
                            <input
                              type="checkbox"
                              className="h-4 w-4 rounded border-input"
                              checked={checked}
                              onChange={(e) => {
                                if (e.target.checked) {
                                  setSelectedNodes([...selectedNodes, node.id]);
                                } else {
                                  setSelectedNodes(
                                    selectedNodes.filter(
                                      (id) => id !== node.id,
                                    ),
                                  );
                                }
                              }}
                            />
                            <strong>{node.hostname}</strong>{' '}
                            {node.is_local && (
                              <span className="text-primary">(Local)</span>
                            )}
                          </label>
                          {checked && (
                            <div className="mt-2 ml-6 space-y-2">
                              <Label className="text-xs text-muted-foreground">
                                Device on {node.hostname}
                              </Label>
                              <Select
                                value={deviceByNode[node.id] ?? ''}
                                onValueChange={(value) =>
                                  setDeviceByNode((prev) => ({
                                    ...prev,
                                    [node.id]: value,
                                  }))
                                }
                              >
                                <SelectTrigger>
                                  <SelectValue placeholder="Select a disk" />
                                </SelectTrigger>
                                <SelectContent>
                                  {(disksByNode[node.id] || []).map((disk) => (
                                    <SelectItem
                                      key={disk.path}
                                      value={disk.path}
                                    >
                                      {disk.path} ({disk.size_human})
                                    </SelectItem>
                                  ))}
                                </SelectContent>
                              </Select>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
          </form>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setModalVisible(false)}
              disabled={createLoading}
            >
              Cancel
            </Button>
            <Button
              type="submit"
              form="create-pool-form"
              disabled={createLoading}
            >
              {createLoading && <Spinner className="mr-2 h-4 w-4" />}
              OK
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
