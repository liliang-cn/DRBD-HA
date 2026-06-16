import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { cn } from '@/lib/utils';
import { useWizardWatch, type WizardFormInstance } from '@/lib/wizard-form';
import type { BlockDevice, Node } from '@/types';

interface StorageConfigStepProps {
  form: WizardFormInstance;
  nodes: Node[];
  availableDisks: Record<string, BlockDevice[]>;
  resources?: Array<{ name: string; id?: string }>;
  onUseExisting?: () => void;
}

const FS_OPTIONS = ['xfs', 'ext4', 'btrfs'];

function FieldLabel({
  children,
  required,
  tooltip,
}: {
  children: React.ReactNode;
  required?: boolean;
  tooltip?: string;
}) {
  return (
    <Label className="flex items-center gap-1" title={tooltip}>
      {children}
      {required && <span className="text-destructive">*</span>}
    </Label>
  );
}

function looksLikeLv(path: string) {
  if (!path) return false;
  if (path.includes('/mapper/')) return true;
  if (!path.startsWith('/dev/')) return true; // e.g. "myvg/mylv"
  const parts = path.split('/').filter((p) => p);
  if (parts.length >= 3 && parts[0] === 'dev') return true;
  return false;
}

export function StorageConfigStep({
  form,
  nodes,
  availableDisks,
  resources = [],
  onUseExisting,
}: StorageConfigStepProps) {
  // Force re-render on relevant field changes by tracking local mirrors.
  const [, forceUpdate] = useState({});
  const rerender = () => forceUpdate({});

  // Set initial defaults once (mirrors the original Form.Item initialValue).
  useEffect(() => {
    const defaults: Record<string, unknown> = {
      port: form.getFieldValue('port') ?? 7788,
      fs_type: form.getFieldValue('fs_type') ?? 'xfs',
      storage_type: form.getFieldValue('storage_type') ?? 'none',
      lvm_allocation_policy:
        form.getFieldValue('lvm_allocation_policy') ?? 'thin',
      lvm_lv_size: form.getFieldValue('lvm_lv_size') ?? '100%FREE',
      lvm_thin_pool_name:
        form.getFieldValue('lvm_thin_pool_name') ?? 'thinpool',
      lvm_thin_pool_size: form.getFieldValue('lvm_thin_pool_size') ?? '1G',
      zfs_thin_volume: form.getFieldValue('zfs_thin_volume') ?? true,
      zfs_volume_size_gb: form.getFieldValue('zfs_volume_size_gb') ?? 10,
      protocol: form.getFieldValue('protocol') ?? 'C',
      verify_alg: form.getFieldValue('verify_alg') ?? 'sha256',
      max_epoch_size: form.getFieldValue('max_epoch_size') ?? 2048,
      after_sb_0pri: form.getFieldValue('after_sb_0pri') ?? 'disconnect',
      after_sb_1pri: form.getFieldValue('after_sb_1pri') ?? 'disconnect',
      after_sb_2pri: form.getFieldValue('after_sb_2pri') ?? 'disconnect',
      force: form.getFieldValue('force') ?? false,
    };
    form.setFieldsValue(defaults);
    rerender();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const nodeDisks = useWizardWatch('node_disks', form) || {};
  const storageType = useWizardWatch('storage_type', form) ?? 'none';
  const allocationPolicy =
    useWizardWatch('lvm_allocation_policy', form) ?? 'thin';

  const set = (name: string, value: unknown) => {
    form.setFieldValue(name, value);
    rerender();
  };
  const setPath = (name: string[], value: unknown) => {
    form.setFieldValue(name, value);
    rerender();
  };
  const get = (name: string) => form.getFieldValue(name);

  const allPaths = Object.values(nodeDisks).filter(
    (v: unknown): v is string => typeof v === 'string' && !!v,
  );
  const hasLvPath = allPaths.some(looksLikeLv);

  const [advancedOpen, setAdvancedOpen] = useState(false);

  return (
    <Card className="w-full">
      <CardHeader>
        <div className="flex items-center justify-between w-full">
          <CardTitle className="text-lg font-semibold">
            Step 2: Storage Configuration
          </CardTitle>
          {resources.length > 0 && onUseExisting && (
            <Button variant="link" onClick={onUseExisting}>
              Use existing DRBD resource →
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent>
        <div className="p-4 space-y-4">
          <div className="space-y-1.5">
            <FieldLabel required>Resource Name</FieldLabel>
            <Input
              placeholder="ha-data"
              value={(get('name') as string) ?? ''}
              onChange={(e) => set('name', e.target.value)}
            />
          </div>

          <div className="space-y-1.5">
            <FieldLabel required>DRBD Port</FieldLabel>
            <Input
              type="number"
              min={1024}
              max={65535}
              value={(get('port') as number) ?? 7788}
              onChange={(e) => set('port', Number(e.target.value))}
            />
          </div>

          <div className="space-y-1.5">
            <FieldLabel>Filesystem Type</FieldLabel>
            <Select
              value={(get('fs_type') as string) ?? 'xfs'}
              onValueChange={(v) => set('fs_type', v)}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {FS_OPTIONS.map((fs) => (
                  <SelectItem key={fs} value={fs}>
                    {fs}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="flex items-center gap-2">
            <hr className="flex-1 border-border" />
            <span className="text-sm text-muted-foreground">Node Disks</span>
            <hr className="flex-1 border-border" />
          </div>

          {nodes.map((node) => {
            const diskOptions = (availableDisks[node.id] || []).map((d) => ({
              value: d.path,
              label: `${d.path} (${d.size_human})`,
            }));
            const listId = `disks-${node.id}`;
            const value =
              (form.getFieldValue(['node_disks', node.id]) as string) ?? '';
            return (
              <div key={node.id} className="space-y-1.5">
                <FieldLabel
                  required
                  tooltip="Select from available disks or enter a custom path (e.g., /dev/sdb, vg/lv1)"
                >
                  {`${node.hostname} (${node.ip})`}
                </FieldLabel>
                <Input
                  list={listId}
                  placeholder="Select disk or enter custom path (e.g., vg/lv1)"
                  value={value}
                  onChange={(e) => {
                    setPath(['node_disks', node.id], e.target.value);
                    // When disk changes, check if any node uses LV paths
                    const allDiskValues =
                      form.getFieldsValue().node_disks || {};
                    const anyHasLvPath = Object.values(allDiskValues).some(
                      (v) =>
                        typeof v === 'string' && v && !v.startsWith('/dev/'),
                    );
                    form.setFieldValue('_has_lv_paths', anyHasLvPath);
                  }}
                />
                <datalist id={listId}>
                  {diskOptions.map((o) => (
                    <option key={o.value} value={o.value}>
                      {o.label}
                    </option>
                  ))}
                </datalist>
              </div>
            );
          })}

          {hasLvPath ? (
            <div className="text-sm text-muted-foreground mb-4">
              ℹ️ <strong>Using existing Logical Volume</strong> - Storage pool
              initialization is skipped when using existing volume paths (e.g.
              VG/LV).
            </div>
          ) : (
            <>
              <div className="flex items-center gap-2">
                <span className="text-sm text-muted-foreground">
                  Storage Pool Initialization (Optional)
                </span>
                <hr className="flex-1 border-border" />
              </div>
              <div className="space-y-1.5">
                <FieldLabel tooltip="Choose storage pool type for selected disks (will wipe data!)">
                  Storage Type
                </FieldLabel>
                <div className="flex flex-col gap-2">
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="radio"
                      name="storage_type"
                      checked={storageType === 'none'}
                      onChange={() => set('storage_type', 'none')}
                    />
                    None (Use raw disks)
                  </label>
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="radio"
                      name="storage_type"
                      checked={storageType === 'lvm'}
                      onChange={() => set('storage_type', 'lvm')}
                    />
                    LVM Storage Pool
                  </label>
                </div>
              </div>
            </>
          )}

          {storageType === 'lvm' && (
            <>
              <div className="space-y-1.5">
                <FieldLabel tooltip="Choose allocation strategy">
                  Allocation Policy
                </FieldLabel>
                <div className="flex flex-col gap-2">
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="radio"
                      name="lvm_allocation_policy"
                      checked={allocationPolicy === 'thin'}
                      onChange={() => set('lvm_allocation_policy', 'thin')}
                    />
                    Thin Provisioning (Snapshots/SSD)
                  </label>
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="radio"
                      name="lvm_allocation_policy"
                      checked={allocationPolicy === 'thick'}
                      onChange={() => set('lvm_allocation_policy', 'thick')}
                    />
                    Standard/Thick (Performance/HDD)
                  </label>
                </div>
              </div>

              <div className="grid grid-cols-3 gap-4">
                <div className="space-y-1.5">
                  <FieldLabel required>Volume Group Name</FieldLabel>
                  <Input
                    placeholder="drbd_vg"
                    value={(get('lvm_vg_name') as string) ?? ''}
                    onChange={(e) => set('lvm_vg_name', e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <FieldLabel tooltip="Defaults to resource name">
                    Logical Volume Name
                  </FieldLabel>
                  <Input
                    placeholder="drbd_lv"
                    value={(get('lvm_lv_name') as string) ?? ''}
                    onChange={(e) => set('lvm_lv_name', e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <FieldLabel tooltip="e.g. 10G, 100%FREE (virtual size for thin volume)">
                    LV Size
                  </FieldLabel>
                  <Input
                    value={(get('lvm_lv_size') as string) ?? '100%FREE'}
                    onChange={(e) => set('lvm_lv_size', e.target.value)}
                  />
                </div>
              </div>

              {allocationPolicy === 'thin' ? (
                <>
                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-1.5">
                      <FieldLabel tooltip="LVM thin pool for efficient storage allocation">
                        Thin Pool Name
                      </FieldLabel>
                      <Input
                        placeholder="thinpool"
                        value={(get('lvm_thin_pool_name') as string) ?? ''}
                        onChange={(e) =>
                          set('lvm_thin_pool_name', e.target.value)
                        }
                      />
                    </div>
                    <div className="space-y-1.5">
                      <FieldLabel tooltip="Metadata size for thin pool (1G supports ~6400 volumes)">
                        Thin Pool Metadata Size
                      </FieldLabel>
                      <Input
                        placeholder="1G"
                        value={(get('lvm_thin_pool_size') as string) ?? ''}
                        onChange={(e) =>
                          set('lvm_thin_pool_size', e.target.value)
                        }
                      />
                    </div>
                  </div>
                  <div className="text-sm text-muted-foreground mb-4">
                    ℹ️ <strong>Thin provisioning enabled</strong>: Volumes will
                    use only the space they actually need. Thin pool metadata
                    size can be increased later if needed for more volumes.
                  </div>
                </>
              ) : (
                <div className="text-sm text-muted-foreground mb-4">
                  ℹ️ <strong>Standard (Thick) provisioning</strong>: Volume will
                  allocate physical space immediately. Best for HDD performance
                  and avoiding metadata overhead.
                </div>
              )}
            </>
          )}

          {storageType === 'zfs' && (
            <>
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1.5">
                  <FieldLabel required tooltip="Name of the ZFS pool to create">
                    ZFS Pool Name
                  </FieldLabel>
                  <Input
                    placeholder="drbd_pool"
                    value={(get('zfs_pool_name') as string) ?? ''}
                    onChange={(e) => set('zfs_pool_name', e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <FieldLabel
                    required
                    tooltip="Virtual size (actual space used depends on data)"
                  >
                    Virtual Volume Size (GB)
                  </FieldLabel>
                  <Input
                    type="number"
                    min={1}
                    value={(get('zfs_volume_size_gb') as number) ?? 10}
                    onChange={(e) =>
                      set('zfs_volume_size_gb', Number(e.target.value))
                    }
                  />
                </div>
              </div>
              <div className="space-y-1.5">
                <FieldLabel tooltip="Defaults to resource name">
                  ZFS Volume Name
                </FieldLabel>
                <Input
                  placeholder="drbd_volume"
                  value={(get('zfs_volume_name') as string) ?? ''}
                  onChange={(e) => set('zfs_volume_name', e.target.value)}
                />
              </div>
              <div className="text-sm text-muted-foreground mb-4">
                ℹ️ <strong>Thin provisioning enabled</strong>: ZFS sparse volumes
                allocate space on-demand. The volume size is virtual; actual
                disk usage will grow as data is written.
              </div>
            </>
          )}

          <div className="flex items-center gap-2">
            <span className="text-sm text-muted-foreground">
              DRBD Advanced Options
            </span>
            <hr className="flex-1 border-border" />
          </div>

          <div className="rounded-lg border border-border">
            <button
              type="button"
              onClick={() => setAdvancedOpen((o) => !o)}
              className="flex w-full items-center justify-between px-4 py-3 text-left font-semibold"
            >
              <span>DRBD Network & Split-Brain Policies</span>
              <span className="text-muted-foreground">
                {advancedOpen ? '▾' : '▸'}
              </span>
            </button>
            {advancedOpen && (
              <div className="space-y-4 px-4 pb-4">
                <div className="rounded border border-blue-500/30 bg-blue-500/10 p-3">
                  <p className="text-xs text-muted-foreground">
                    ℹ️ These options control DRBD behavior during network
                    partitions and split-brain scenarios. For two-node clusters,{' '}
                    <strong>preferred-nodes</strong> in HA Config is typically
                    sufficient. These policies provide additional automatic
                    recovery mechanisms.
                  </p>
                </div>

                <div className="grid grid-cols-3 gap-4">
                  <div className="space-y-1.5">
                    <FieldLabel>Replication Protocol</FieldLabel>
                    <Select
                      value={(get('protocol') as string) ?? 'C'}
                      onValueChange={(v) => set('protocol', v)}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="A">A (Async)</SelectItem>
                        <SelectItem value="B">B (Semi-sync)</SelectItem>
                        <SelectItem value="C">C (Sync)</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-1.5">
                    <FieldLabel>Data Integrity</FieldLabel>
                    <Select
                      value={(get('verify_alg') as string) ?? 'sha256'}
                      onValueChange={(v) => set('verify_alg', v)}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="none">None</SelectItem>
                        <SelectItem value="crc32c">CRC32C (Fast)</SelectItem>
                        <SelectItem value="sha1">SHA1</SelectItem>
                        <SelectItem value="sha256">SHA256 (Secure)</SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="space-y-1.5">
                    <FieldLabel>Max Epoch Size</FieldLabel>
                    <Input
                      type="number"
                      min={1}
                      max={20000}
                      value={(get('max_epoch_size') as number) ?? 2048}
                      onChange={(e) =>
                        set('max_epoch_size', Number(e.target.value))
                      }
                    />
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">
                    Split-Brain Recovery Policies
                  </span>
                  <hr className="flex-1 border-border" />
                </div>

                <div className="rounded border border-yellow-500/30 bg-yellow-500/10 p-3 mb-4">
                  <p className="text-xs text-muted-foreground">
                    ⚠️ <strong>Split-brain</strong> occurs when both nodes become
                    Primary due to network partition. These policies define
                    automatic recovery. For manual recovery, use the "Recover
                    Split-Brain" action.
                  </p>
                </div>

                <div className="space-y-1.5">
                  <FieldLabel>After-SB-0Pri (Both Secondary)</FieldLabel>
                  <Select
                    value={(get('after_sb_0pri') as string) ?? 'disconnect'}
                    onValueChange={(v) => set('after_sb_0pri', v)}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="disconnect">
                        disconnect (Manual recovery required)
                      </SelectItem>
                      <SelectItem value="discard-zero-changes">
                        discard-zero-changes (Auto-discard unchanged data)
                      </SelectItem>
                      <SelectItem value="call-pri-lost-after-sb">
                        call-pri-lost-after-sb (Call recovery script)
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-1.5">
                  <FieldLabel>After-SB-1Pri (One Primary)</FieldLabel>
                  <Select
                    value={(get('after_sb_1pri') as string) ?? 'disconnect'}
                    onValueChange={(v) => set('after_sb_1pri', v)}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="disconnect">
                        disconnect (Manual recovery required)
                      </SelectItem>
                      <SelectItem value="consensus">
                        consensus (Requires both nodes to agree)
                      </SelectItem>
                      <SelectItem value="call-pri-lost-after-sb">
                        call-pri-lost-after-sb (Call recovery script)
                      </SelectItem>
                      <SelectItem value="violently-as-0pri">
                        violently-as-0pri (Force both to Secondary)
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>

                <div className="space-y-1.5">
                  <FieldLabel>After-SB-2Pri (Both Primary)</FieldLabel>
                  <Select
                    value={(get('after_sb_2pri') as string) ?? 'disconnect'}
                    onValueChange={(v) => set('after_sb_2pri', v)}
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="disconnect">
                        disconnect (Safest - Manual recovery)
                      </SelectItem>
                      <SelectItem value="violently-as-0pri">
                        violently-as-0pri (Force both to Secondary - DATA LOSS
                        RISK)
                      </SelectItem>
                      <SelectItem value="call-pri-lost-after-sb">
                        call-pri-lost-after-sb (Call recovery script)
                      </SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
            )}
          </div>

          <hr className="border-border my-4" />
          <label
            className={cn('flex items-center gap-2 text-sm text-destructive')}
            title="Bypass safety checks (e.g. if device is already configured)"
          >
            <input
              type="checkbox"
              checked={(get('force') as boolean) ?? false}
              onChange={(e) => set('force', e.target.checked)}
            />
            Force creation (ignore safety checks)
          </label>
        </div>
      </CardContent>
    </Card>
  );
}
