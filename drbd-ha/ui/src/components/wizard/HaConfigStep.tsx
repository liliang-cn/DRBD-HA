import { Eye, EyeOff, Settings } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  OcfAgentEditor,
  type OcfAgentPayload,
} from '@/components/ha/OcfAgentEditor';
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
import { useWizardWatch, type WizardFormInstance } from '@/lib/wizard-form';
import type { HaType, ServiceFileInfo } from '@/types';

type ConfigMode = 'simple' | 'advanced';

// Define Resource type locally
interface Resource {
  name: string;
  id?: string;
}

interface HaConfigStepProps {
  form: WizardFormInstance;
  mode?: 'service' | 'storage';
  haType: HaType;
  onHaTypeChange: (type: HaType) => void;
  storageStrategy: 'raw' | 'lvm';
  resources: Resource[];
  services: ServiceFileInfo[];
}

const FS_OPTIONS = ['xfs', 'ext4', 'btrfs'];

function FieldLabel({ children }: { children: React.ReactNode }) {
  return <Label>{children}</Label>;
}

export function HaConfigStep({ form, resources, services }: HaConfigStepProps) {
  const [configMode, setConfigMode] = useState<ConfigMode>('simple');
  const [showTomlPreview, setShowTomlPreview] = useState(true);
  const [ocfAgents, setOcfAgents] = useState<OcfAgentPayload[]>([]);
  const [, forceUpdate] = useState({});
  const rerender = () => forceUpdate({});

  // Split pane state
  const [leftPanelWidth, setLeftPanelWidth] = useState(50); // percentage
  const [_isDragging, setIsDragging] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const startXRef = useRef(0);
  const startWidthRef = useRef(50);

  // Set initial defaults once (mirrors the original Form.Item initialValue).
  useEffect(() => {
    const defaults: Record<string, unknown> = {
      mount_strategy: form.getFieldValue('mount_strategy') ?? 'systemd',
      fs_type: form.getFieldValue('fs_type') ?? 'xfs',
      vip_netmask: form.getFieldValue('vip_netmask') ?? 24,
      preferred_nodes_policy:
        form.getFieldValue('preferred_nodes_policy') ?? 'always',
      sleep_before_promote_factor:
        form.getFieldValue('sleep_before_promote_factor') ?? 1,
      on_quorum_loss: form.getFieldValue('on_quorum_loss') ?? 'shutdown',
      on_demote_failure: form.getFieldValue('on_demote_failure') ?? 'reboot',
      dependencies_as: form.getFieldValue('dependencies_as') ?? 'Requires',
      target_as: form.getFieldValue('target_as') ?? 'Requires',
      migrate_data: form.getFieldValue('migrate_data') ?? false,
      format_device: form.getFieldValue('format_device') ?? true,
      preserve_permissions: form.getFieldValue('preserve_permissions') ?? true,
    };
    form.setFieldsValue(defaults);
    rerender();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Handle drag start
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    setIsDragging(true);
    startXRef.current = e.clientX;
    startWidthRef.current = leftPanelWidth;

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);
  };

  // Handle drag move
  const handleMouseMove = (e: MouseEvent) => {
    if (!containerRef.current) return;

    const containerRect = containerRef.current.getBoundingClientRect();
    const containerWidth = containerRect.width;
    const deltaX = e.clientX - startXRef.current;
    const deltaPercent = (deltaX / containerWidth) * 100;

    let newWidth = startWidthRef.current + deltaPercent;
    newWidth = Math.max(20, Math.min(80, newWidth));

    setLeftPanelWidth(newWidth);
  };

  // Handle drag end
  const handleMouseUp = () => {
    setIsDragging(false);
    document.removeEventListener('mousemove', handleMouseMove);
    document.removeEventListener('mouseup', handleMouseUp);
  };

  // Watch form values for TOML preview - watch individual fields for reactivity
  const resourceName = useWizardWatch('resource_name', form);
  const mountPoint = useWizardWatch('mount_point', form);
  const fsType = useWizardWatch('fs_type', form);
  const selectedServices = useWizardWatch('services', form);
  const vipAddress = useWizardWatch('vip_address', form);
  const vipNetmask = useWizardWatch('vip_netmask', form);
  const preferredNodes = useWizardWatch('preferred_nodes', form);
  const preferredNodesPolicy = useWizardWatch('preferred_nodes_policy', form);
  const onQuorumLoss = useWizardWatch('on_quorum_loss', form);
  const onDemoteFailure = useWizardWatch('on_demote_failure', form);
  const stopOnDemote = useWizardWatch('stop_on_demote', form);
  const dependenciesAs = useWizardWatch('dependencies_as', form);
  const targetAs = useWizardWatch('target_as', form);
  const sleepBeforePromote = useWizardWatch(
    'sleep_before_promote_factor',
    form,
  );
  const migrateData = useWizardWatch('migrate_data', form);

  const get = (name: string) => form.getFieldValue(name);
  const set = (name: string, value: unknown) => {
    form.setFieldValue(name, value);
    rerender();
  };

  // Generate drbd-reactor TOML preview
  const tomlPreview = useMemo(() => {
    const resName = (resourceName as string) || 'r0';
    const mntPoint = (mountPoint as string) || '/mnt/data';
    const svc = (selectedServices as string[]) || [];
    const vipAddr = vipAddress;
    const vipNm = vipNetmask || 24;
    const agents = ocfAgents || [];
    const prefNodes = preferredNodes;
    const prefPolicy = preferredNodesPolicy;
    const quorumLoss = onQuorumLoss;
    const stopOnExit = stopOnDemote !== false;
    const depsAs = dependenciesAs;
    const tgtAs = targetAs;
    const sleepFactor = sleepBeforePromote;

    // Parse preferred_nodes string or array
    let preferredNodesList: string[] = [];
    if (typeof prefNodes === 'string') {
      preferredNodesList = prefNodes.split(/[\s,]+/).filter((n) => n);
    } else if (Array.isArray(prefNodes)) {
      preferredNodesList = prefNodes;
    }

    // Convert mount point to systemd mount unit name
    const mountUnitName = `${mntPoint
      .slice(1)
      .replace(/-/g, '--')
      .replace(/\//g, '-')}.mount`;

    // Build start array
    const startEntries: string[] = [];

    if (configMode === 'simple') {
      startEntries.push(`    "${mountUnitName}",`);
      startEntries.push('');

      if (vipAddr) {
        startEntries.push(
          `    "ocf:heartbeat:IPaddr2 ${resName}_vip ip=${vipAddr} cidr_netmask=${vipNm}",`,
        );
        startEntries.push('');
      }

      svc.forEach((service: string) => {
        startEntries.push(`    "${service}",`);
      });
    } else {
      agents.forEach((agent: OcfAgentPayload) => {
        if (!agent) return;

        if (agent.name && agent.instance_name) {
          const params = Object.entries(agent.params || {})
            .map(([k, v]) => `${k}=${v}`)
            .join(' ');
          startEntries.push(
            `    "${agent.name} ${agent.instance_name}${params ? ` ${params}` : ''}",`,
          );
        }
      });
    }

    const startArrayContent = startEntries.join('\n');
    const startArray = `start = [

${startArrayContent}

]`;

    const advancedOptions: string[] = [];
    if (depsAs) advancedOptions.push(`dependencies-as = "${depsAs}"`);
    if (tgtAs) advancedOptions.push(`target-as = "${tgtAs}"`);
    if (quorumLoss) advancedOptions.push(`on-quorum-loss = "${quorumLoss}"`);
    if (preferredNodesList.length > 0) {
      const nodes = preferredNodesList.map((n) => `"${n}"`).join(', ');
      advancedOptions.push(`preferred-nodes = [${nodes}]`);
    }
    if (prefPolicy)
      advancedOptions.push(`preferred-nodes-policy = "${prefPolicy}"`);
    if (sleepFactor)
      advancedOptions.push(`sleep-before-promote-factor = ${sleepFactor}`);

    const advancedSection =
      advancedOptions.length > 0 ? `\n${advancedOptions.join('\n')}` : '';

    const mountStrategyComment =
      configMode === 'simple' ? '\n\n# Mount strategy: systemd' : '';

    return `# drbd-reactor promoter configuration
# Generated by drbd-ha

[[promoter]]
[promoter.resources.${resName}]
${startArray}
stop-services-on-exit = ${stopOnExit}${advancedSection}${mountStrategyComment}
`;
  }, [
    resourceName,
    mountPoint,
    fsType,
    selectedServices,
    vipAddress,
    vipNetmask,
    ocfAgents,
    preferredNodes,
    preferredNodesPolicy,
    onQuorumLoss,
    onDemoteFailure,
    stopOnDemote,
    dependenciesAs,
    targetAs,
    sleepBeforePromote,
    configMode,
  ]);

  // Tags-mode service selection fallback: combine known services + free entry.
  const servicesValue: string[] = Array.isArray(selectedServices)
    ? selectedServices
    : [];
  const [serviceInput, setServiceInput] = useState('');
  const addService = (name: string) => {
    let v = name.trim();
    if (!v) return;
    // Normalize to a full systemd unit name; the backend rejects bare names.
    // Default to .service when no known unit-type suffix is present.
    const unitSuffixes = [
      '.service',
      '.target',
      '.socket',
      '.mount',
      '.timer',
      '.path',
      '.slice',
      '.scope',
      '.device',
      '.swap',
      '.automount',
    ];
    if (!unitSuffixes.some((s) => v.endsWith(s))) {
      v = `${v}.service`;
    }
    if (servicesValue.includes(v)) return;
    set('services', [...servicesValue, v]);
    setServiceInput('');
  };
  const removeService = (name: string) => {
    set(
      'services',
      servicesValue.filter((s) => s !== name),
    );
  };

  const preferredNodesValue = (get('preferred_nodes') as string) || '';

  return (
    <Card className="w-full">
      <CardHeader>
        <div className="flex items-center gap-2">
          <CardTitle>Step 3: Configure Service HA</CardTitle>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => setShowTomlPreview(!showTomlPreview)}
            title={showTomlPreview ? 'Hide preview' : 'Show preview'}
          >
            {showTomlPreview ? (
              <EyeOff className="h-4 w-4" />
            ) : (
              <Eye className="h-4 w-4" />
            )}
          </Button>
        </div>
      </CardHeader>
      <CardContent>
        <div
          ref={containerRef}
          style={{
            display: 'flex',
            alignItems: 'stretch',
            minHeight: 600,
            gap: 0,
          }}
        >
          {/* Left: Form */}
          <div
            style={{
              flex: showTomlPreview ? `0 0 ${leftPanelWidth}%` : '1 1 100%',
              minWidth: showTomlPreview ? '20%' : 'auto',
              overflowY: 'visible',
              overflowX: 'hidden',
              paddingRight: showTomlPreview ? 16 : 0,
            }}
          >
            <div className="space-y-4">
              <div className="space-y-1.5">
                <FieldLabel>Profile Name</FieldLabel>
                <Input
                  placeholder="my-service-ha"
                  value={(get('name') as string) ?? ''}
                  onChange={(e) => set('name', e.target.value)}
                />
              </div>

              <div className="space-y-1.5">
                <FieldLabel>DRBD Resource</FieldLabel>
                <Select
                  value={(get('resource_name') as string) ?? ''}
                  onValueChange={(v) => set('resource_name', v)}
                >
                  <SelectTrigger>
                    <SelectValue placeholder="Select DRBD resource" />
                  </SelectTrigger>
                  <SelectContent>
                    {resources.map((r) => (
                      <SelectItem key={r.name} value={r.name}>
                        {r.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {/* Configuration Mode Selector */}
              <div className="space-y-1.5">
                <FieldLabel>Configuration Mode</FieldLabel>
                <div className="flex gap-4">
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="radio"
                      name="config_mode"
                      checked={configMode === 'simple'}
                      onChange={() => setConfigMode('simple')}
                    />
                    Simple
                  </label>
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="radio"
                      name="config_mode"
                      checked={configMode === 'advanced'}
                      onChange={() => setConfigMode('advanced')}
                    />
                    Advanced (via OCF Agents)
                  </label>
                </div>
              </div>

              {/* Simple Mode: Services, Mount Point, Filesystem, VIP */}
              {configMode === 'simple' && (
                <>
                  <div className="space-y-1.5">
                    <FieldLabel>Systemd Services</FieldLabel>
                    {/* Dropdown to pick a discovered service; opens directly below the field */}
                    <Select value="" onValueChange={(v) => addService(v)}>
                      <SelectTrigger>
                        <SelectValue placeholder="Select a service…" />
                      </SelectTrigger>
                      <SelectContent>
                        {services.filter((s) => !servicesValue.includes(s.name))
                          .length === 0 ? (
                          <div className="px-2 py-1.5 text-sm text-muted-foreground">
                            No services available
                          </div>
                        ) : (
                          services
                            .filter((s) => !servicesValue.includes(s.name))
                            .map((s) => (
                              <SelectItem key={s.name} value={s.name}>
                                {s.name}
                              </SelectItem>
                            ))
                        )}
                      </SelectContent>
                    </Select>
                    {/* Optional free-text for a custom unit name */}
                    <Input
                      placeholder="…or type a custom unit name, press Enter"
                      value={serviceInput}
                      onChange={(e) => setServiceInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') {
                          e.preventDefault();
                          addService(serviceInput);
                        }
                      }}
                      onBlur={() => addService(serviceInput)}
                    />
                    {servicesValue.length > 0 && (
                      <div className="flex flex-wrap gap-2 pt-1">
                        {servicesValue.map((s) => (
                          <button
                            key={s}
                            type="button"
                            onClick={() => removeService(s)}
                            className="inline-flex items-center gap-1 rounded-md bg-primary/10 px-2 py-1 text-xs text-primary"
                          >
                            {s}
                            <span className="opacity-70">×</span>
                          </button>
                        ))}
                      </div>
                    )}
                  </div>

                  <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-1.5">
                      <FieldLabel>Mount Point</FieldLabel>
                      <Input
                        placeholder="/srv/myapp/data"
                        value={(get('mount_point') as string) ?? ''}
                        onChange={(e) => set('mount_point', e.target.value)}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <FieldLabel>Filesystem</FieldLabel>
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
                  </div>

                  <div className="grid grid-cols-3 gap-4">
                    <div className="col-span-2 space-y-1.5">
                      <FieldLabel>VIP Address</FieldLabel>
                      <Input
                        placeholder="192.168.1.100"
                        value={(get('vip_address') as string) ?? ''}
                        onChange={(e) => set('vip_address', e.target.value)}
                      />
                      <p className="text-xs text-muted-foreground">
                        Virtual IP address for failover
                      </p>
                    </div>
                    <div className="space-y-1.5">
                      <FieldLabel>VIP Netmask</FieldLabel>
                      <Input
                        type="number"
                        min={1}
                        max={32}
                        value={(get('vip_netmask') as number) ?? 24}
                        onChange={(e) =>
                          set('vip_netmask', Number(e.target.value))
                        }
                      />
                      <p className="text-xs text-muted-foreground">
                        CIDR notation
                      </p>
                    </div>
                  </div>
                </>
              )}

              {/* Advanced Mode: OCF Agents */}
              {configMode === 'advanced' && (
                <>
                  <div className="flex items-center gap-2">
                    <hr className="flex-1 border-border" />
                    <span className="text-sm text-muted-foreground">
                      OCF Agents
                    </span>
                    <hr className="flex-1 border-border" />
                  </div>
                  <div style={{ marginTop: '16px', marginBottom: '32px' }}>
                    <OcfAgentEditor
                      mode="create"
                      externalForm={form}
                      resources={resources}
                      services={services.map((s) => s.name)}
                      onAgentsChange={setOcfAgents}
                    />
                  </div>
                </>
              )}

              <div className="flex items-center gap-2">
                <hr className="flex-1 border-border" />
                <span className="text-sm text-muted-foreground">
                  Advanced Options
                </span>
                <hr className="flex-1 border-border" />
              </div>

              {/* --- Advanced Options --- */}
              <AdvancedOptions>
                <div className="space-y-6">
                  {/* Preferred Nodes Configuration */}
                  <div className="bg-muted p-4 rounded-md border border-border">
                    <h4 className="font-semibold mb-3 text-foreground">
                      Node Preferences
                    </h4>
                    <p className="block mb-4 text-xs text-muted-foreground">
                      Enter preferred nodes for running this service (comma or
                      space separated). The first node has the highest priority.
                    </p>

                    <div className="space-y-1.5">
                      <FieldLabel>Preferred Nodes</FieldLabel>
                      <Input
                        placeholder="Enter hostnames separated by commas or spaces"
                        value={preferredNodesValue}
                        onChange={(e) => set('preferred_nodes', e.target.value)}
                      />
                      <p className="text-xs text-muted-foreground">
                        Example: orange1, orange2
                      </p>
                    </div>

                    {preferredNodesValue && (
                      <>
                        <div className="mt-4 space-y-1.5">
                          <FieldLabel>Preferred Nodes Policy</FieldLabel>
                          <div className="flex flex-col gap-2">
                            <label className="flex items-center gap-2 text-sm">
                              <input
                                type="radio"
                                name="preferred_nodes_policy"
                                checked={
                                  (get('preferred_nodes_policy') as string) ===
                                  'always'
                                }
                                onChange={() =>
                                  set('preferred_nodes_policy', 'always')
                                }
                              />
                              Always prefer higher priority
                            </label>
                            <label className="flex items-center gap-2 text-sm">
                              <input
                                type="radio"
                                name="preferred_nodes_policy"
                                checked={
                                  (get('preferred_nodes_policy') as string) ===
                                  'start-only'
                                }
                                onChange={() =>
                                  set('preferred_nodes_policy', 'start-only')
                                }
                              />
                              Start-only preference
                            </label>
                          </div>
                          <p className="text-xs text-muted-foreground">
                            <strong>Always:</strong> Always migrate to higher
                            priority nodes when they become available.{' '}
                            <strong>Start-only:</strong> Only consider priority
                            during initial service startup.
                          </p>
                        </div>

                        <div className="mt-4 space-y-1.5">
                          <FieldLabel>Promotion Delay Factor</FieldLabel>
                          <div className="flex items-center gap-2">
                            <Input
                              type="number"
                              min={1}
                              max={10}
                              value={
                                (get(
                                  'sleep_before_promote_factor',
                                ) as number) ?? 1
                              }
                              onChange={(e) =>
                                set(
                                  'sleep_before_promote_factor',
                                  Number(e.target.value),
                                )
                              }
                            />
                            <span className="text-muted-foreground">×</span>
                          </div>
                          <p className="text-xs text-muted-foreground">
                            Multiplier for promotion delay based on node
                            priority. Higher values increase the delay between
                            priority levels.
                          </p>
                        </div>
                      </>
                    )}
                  </div>

                  {/* Quorum and Failure Handling */}
                  <div className="bg-yellow-500/10 p-4 rounded-md border border-yellow-500/20">
                    <h4 className="font-semibold mb-3 text-foreground">
                      Quorum & Failure Handling
                    </h4>

                    <div className="space-y-1.5">
                      <FieldLabel>On Quorum Loss</FieldLabel>
                      <Select
                        value={(get('on_quorum_loss') as string) ?? 'shutdown'}
                        onValueChange={(v) => set('on_quorum_loss', v)}
                      >
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="shutdown">
                            Shutdown (Default)
                          </SelectItem>
                          <SelectItem value="freeze">
                            Freeze services
                          </SelectItem>
                          <SelectItem value="ignore">
                            Ignore (Not recommended)
                          </SelectItem>
                        </SelectContent>
                      </Select>
                      <p className="text-xs text-muted-foreground">
                        Action to take when DRBD quorum is lost
                      </p>
                    </div>

                    <div className="mt-4 space-y-1.5">
                      <FieldLabel>On DRBD Demote Failure</FieldLabel>
                      <Select
                        value={(get('on_demote_failure') as string) ?? 'reboot'}
                        onValueChange={(v) => set('on_demote_failure', v)}
                      >
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="reboot-immediate">
                            Reboot immediately
                          </SelectItem>
                          <SelectItem value="reboot">
                            Reboot (graceful)
                          </SelectItem>
                          <SelectItem value="poweroff">Power off</SelectItem>
                          <SelectItem value="ignore">
                            Ignore (Not recommended)
                          </SelectItem>
                        </SelectContent>
                      </Select>
                      <p className="text-xs text-muted-foreground">
                        Action to take when DRBD demotion fails
                      </p>
                    </div>
                  </div>

                  {/* Dependency Configuration */}
                  <div className="bg-blue-500/10 p-4 rounded-md border border-blue-500/20">
                    <h4 className="font-semibold mb-3 text-foreground">
                      Service Dependencies
                    </h4>

                    <div className="grid grid-cols-2 gap-4">
                      <div className="space-y-1.5">
                        <FieldLabel>Service Dependencies</FieldLabel>
                        <Select
                          value={
                            (get('dependencies_as') as string) ?? 'Requires'
                          }
                          onValueChange={(v) => set('dependencies_as', v)}
                        >
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="Requires">
                              Requires (Strict)
                            </SelectItem>
                            <SelectItem value="Wants">
                              Wants (Flexible)
                            </SelectItem>
                            <SelectItem value="Requisite">Requisite</SelectItem>
                          </SelectContent>
                        </Select>
                        <p className="text-xs text-muted-foreground">
                          Type of dependency between services
                        </p>
                      </div>
                      <div className="space-y-1.5">
                        <FieldLabel>Target Dependencies</FieldLabel>
                        <Select
                          value={(get('target_as') as string) ?? 'Requires'}
                          onValueChange={(v) => set('target_as', v)}
                        >
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            <SelectItem value="Requires">
                              Requires (Strict)
                            </SelectItem>
                            <SelectItem value="Wants">
                              Wants (Flexible)
                            </SelectItem>
                            <SelectItem value="Requisite">Requisite</SelectItem>
                          </SelectContent>
                        </Select>
                        <p className="text-xs text-muted-foreground">
                          Type of dependency for the target unit
                        </p>
                      </div>
                    </div>
                  </div>
                </div>
              </AdvancedOptions>

              {/* Data Migration for Generic HA */}
              <div className="flex items-center gap-2">
                <hr className="flex-1 border-border" />
                <span className="text-sm text-muted-foreground">
                  Data Migration
                </span>
                <hr className="flex-1 border-border" />
              </div>
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={(get('migrate_data') as boolean) ?? false}
                  onChange={(e) => {
                    const checked = e.target.checked;
                    set('migrate_data', checked);
                    if (checked && !get('source_path')) {
                      form.setFieldValue('source_path', get('mount_point'));
                    }
                  }}
                />
                Migrate existing data to shared storage
              </label>

              {migrateData ? (
                <div className="bg-muted p-4 rounded-md mb-4 border border-border">
                  <p className="block mb-4 text-xs text-muted-foreground">
                    This will copy data from the source directory to the new
                    DRBD volume. Services might need to be stopped during this
                    process.
                  </p>
                  <div className="space-y-1.5">
                    <FieldLabel>Source Directory</FieldLabel>
                    <Input
                      value={(get('source_path') as string) ?? ''}
                      onChange={(e) => set('source_path', e.target.value)}
                    />
                  </div>
                  <div className="mt-3 flex gap-6">
                    <label className="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={(get('format_device') as boolean) ?? true}
                        onChange={(e) => set('format_device', e.target.checked)}
                      />
                      Format device before migration
                    </label>
                    <label className="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={
                          (get('preserve_permissions') as boolean) ?? true
                        }
                        onChange={(e) =>
                          set('preserve_permissions', e.target.checked)
                        }
                      />
                      Preserve permissions
                    </label>
                  </div>
                </div>
              ) : null}
            </div>
          </div>

          {/* Drag Handle - only shown when preview is visible */}
          {showTomlPreview && (
            <div
              onMouseDown={handleMouseDown}
              title="Drag to resize"
              style={{
                cursor: 'col-resize',
                width: 20,
                userSelect: 'none',
                flex: '0 0 auto',
                alignSelf: 'stretch',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <div className="h-full w-px bg-border" />
            </div>
          )}

          {/* Right: TOML Preview */}
          {showTomlPreview && (
            <div
              style={{
                flex: `0 0 ${100 - leftPanelWidth}%`,
                minWidth: '20%',
                overflow: 'auto',
              }}
            >
              <div className="h-full rounded-lg border border-border bg-muted p-4">
                <span className="mb-3 block font-semibold">
                  drbd-reactor TOML Preview
                </span>
                <pre className="m-0 whitespace-pre rounded border border-border bg-background p-3 font-mono text-xs leading-relaxed text-foreground">
                  {tomlPreview || '# Fill in the form to see TOML preview'}
                </pre>
              </div>
            </div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

function AdvancedOptions({ children }: { children: React.ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-2 text-sm font-medium text-foreground"
      >
        <Settings className="h-4 w-4" />
        <span>Advanced Options</span>
        <span className="text-muted-foreground">{open ? '▾' : '▸'}</span>
      </button>
      {open && <div className="mt-4">{children}</div>}
    </div>
  );
}
