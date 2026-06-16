import { useSortable } from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import {
  ChevronRight,
  GripVertical,
  HelpCircle,
  MinusCircle,
  Plus,
  Trash2,
} from 'lucide-react';
import type {
  OcfAgentWithMetadata,
  ParamEntry,
  ResourceAgent,
} from '@/api/ha-profiles';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import type { WizardFormInstance } from '@/lib/wizard-form';
import { useWizardWatch } from '@/lib/wizard-form';

// Helper to get param value from ParamEntry[]
function getParamValue(
  params: ParamEntry[] | undefined,
  key: string,
): string | undefined {
  return params?.find((p) => p.key === key)?.value;
}

// Helper to check if param exists
function hasParam(params: ParamEntry[] | undefined, key: string): boolean {
  return params?.some((p) => p.key === key) || false;
}

export interface SortableItemProps {
  id: string;
  index: number;
  agentWithMeta: OcfAgentWithMetadata;
  metadata: ResourceAgent | null;
  isLoadingMetadata: boolean;
  currentTheme: string;
  form: WizardFormInstance;
  onFieldChange: (index: number, field: 'params' | 'original') => void;
  onDelete: (index: number) => void;
  onExpand: (key: string) => void;
  expandedKeys: Set<string>;
  onRemoveParam: (index: number, paramName: string) => void;
  onAddParam: (index: number) => void;
  addedParams: Map<number, Set<string>>;
}

export function SortableAgentItem({
  id,
  index,
  agentWithMeta,
  metadata,
  isLoadingMetadata,
  currentTheme,
  form,
  onFieldChange,
  onDelete,
  onExpand,
  expandedKeys,
  onRemoveParam,
  onAddParam,
  addedParams,
}: SortableItemProps) {
  // Get instanceId for stable key lookup
  // Fallback to array index if instanceId not set
  const stableKey = (agentWithMeta as any).instanceId ?? index;
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  };

  // Subscribe once to form changes so all controlled fields below re-render
  // when any value changes (the render functions read via form.getFieldValue).
  useWizardWatch(['agents', index], form);

  const { item } = agentWithMeta;
  // Use instanceId for stable panel key across reorders
  const instanceId = (agentWithMeta as any).instanceId ?? index;
  const panelKey = `agent-${instanceId}`;
  const isExpanded = expandedKeys.has(panelKey);

  // Determine if this is an OCF agent or plain systemd unit
  const isOcf = item.is_ocf;
  const ocfAgent = item.ocf_agent;

  // Render form field for plain systemd unit
  const renderPlainUnitField = () => {
    const fieldName = ['agents', index, 'original'];
    const value = form.getFieldValue(fieldName) ?? item.original ?? '';

    return (
      <div className="mt-2 flex flex-col gap-1.5">
        <Label>
          Systemd Unit <span className="text-destructive">*</span>
        </Label>
        <Input
          placeholder="e.g., var-lib-linstor.mount"
          value={value}
          onChange={(e) => {
            form.setFieldValue(fieldName, e.target.value);
            onFieldChange(index, 'original');
          }}
        />
      </div>
    );
  };

  // Render form field based on parameter type (for OCF agents)
  const renderFormField = (param: any) => {
    if (!ocfAgent) return null;

    const fieldName = ['agents', index, 'params', param.name];
    const fallback = getParamValue(ocfAgent.params, param.name);
    const watched = form.getFieldValue(fieldName);
    const currentValue = watched ?? fallback;

    const label = (
      <div className="flex items-center gap-1">
        <span className="text-[13px]">{param.name}</span>
        {param.required && <span className="text-destructive">*</span>}
        {(param.longdesc || param.shortdesc) && (
          <Tooltip>
            <TooltipTrigger asChild>
              <span>
                <HelpCircle className="h-3 w-3 text-muted-foreground" />
              </span>
            </TooltipTrigger>
            <TooltipContent>
              <div className="max-w-[400px] whitespace-pre-wrap">
                {param.longdesc || param.shortdesc}
              </div>
            </TooltipContent>
          </Tooltip>
        )}
      </div>
    );

    const setValue = (v: string) => {
      form.setFieldValue(fieldName, v);
      onFieldChange(index, 'params');
    };

    switch (param.type) {
      case 'integer':
        return (
          <div className="flex flex-col gap-1.5">
            <Label asChild>
              <div>{label}</div>
            </Label>
            <Input
              type="number"
              className="w-full"
              value={currentValue ?? ''}
              onChange={(e) => setValue(e.target.value)}
            />
          </div>
        );

      case 'boolean': {
        const checked =
          currentValue === 'true' ||
          currentValue === '1' ||
          currentValue === 'yes' ||
          currentValue === true;
        return (
          <div className="flex flex-col gap-1.5">
            <Label asChild>
              <div>{label}</div>
            </Label>
            <button
              type="button"
              role="switch"
              aria-checked={checked}
              onClick={() => setValue(checked ? 'false' : 'true')}
              className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                checked ? 'bg-primary' : 'bg-muted'
              }`}
            >
              <span
                className={`inline-block h-4 w-4 transform rounded-full bg-background transition-transform ${
                  checked ? 'translate-x-4' : 'translate-x-0.5'
                }`}
              />
            </button>
          </div>
        );
      }

      default:
        return (
          <div className="flex flex-col gap-1.5">
            <Label asChild>
              <div>{label}</div>
            </Label>
            <Input
              value={currentValue ?? ''}
              onChange={(e) => setValue(e.target.value)}
            />
          </div>
        );
    }
  };

  // Get display info for the item
  const getItemDisplay = () => {
    if (isOcf && ocfAgent) {
      return {
        typeLabel: ocfAgent.agent_type,
        instanceLabel: ocfAgent.instance_name,
      };
    } else {
      // Plain systemd unit
      const name = item.original.split(' ')[0]; // Take first part as name
      return {
        typeLabel: 'systemd',
        instanceLabel: name,
      };
    }
  };

  const displayInfo = getItemDisplay();

  return (
    <div ref={setNodeRef} style={style} className="sortable-agent-item">
      <Card
        className="mb-2 cursor-grab rounded-lg border p-0"
        style={{
          background: currentTheme === 'dark' ? '#1e293b' : '#f8fafc',
          borderColor: currentTheme === 'dark' ? '#334155' : '#e2e8f0',
        }}
      >
        {/* Header - always visible */}
        <div
          className="flex cursor-pointer items-center gap-3 px-4 py-3"
          onClick={() => onExpand(panelKey)}
          {...attributes}
        >
          {/* Drag handle - only this element is draggable */}
          <div
            {...listeners}
            className="flex cursor-grab items-center"
            onClick={(e) => e.stopPropagation()}
          >
            <GripVertical className="h-4 w-4 text-muted-foreground" />
          </div>

          {/* Expand/Collapse icon */}
          <ChevronRight
            className="h-3 w-3 text-muted-foreground transition-transform"
            style={{
              transform: isExpanded ? 'rotate(90deg)' : 'rotate(0deg)',
            }}
          />

          {/* Item info */}
          <div className="flex flex-1 items-center gap-2">
            <Badge variant="secondary">#{index + 1}</Badge>
            {isOcf && <Badge variant="secondary">OCF</Badge>}
            <Badge variant="secondary">{displayInfo.typeLabel}</Badge>
            <Badge variant="secondary">{displayInfo.instanceLabel}</Badge>
            {isOcf && isLoadingMetadata && (
              <Badge variant="outline">Loading metadata...</Badge>
            )}
            {isOcf && !metadata && !isLoadingMetadata && (
              <Badge variant="destructive">Metadata not found</Badge>
            )}
          </div>

          {/* Delete button */}
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-destructive"
            onClick={(e) => {
              e.stopPropagation();
              if (window.confirm('Delete this item?')) {
                onDelete(index);
              }
            }}
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>

        {/* Expanded content */}
        <div
          className="p-4"
          style={{
            borderTop: `1px solid ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
            display: isExpanded ? 'block' : 'none',
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {/* Original string (for reference) */}
          <div className="mb-4">
            <span className="text-sm text-muted-foreground">Original:</span>
            <div
              style={{
                marginTop: '4px',
                padding: '8px',
                background: currentTheme === 'dark' ? '#0f172a' : '#f1f5f9',
                borderRadius: '4px',
                fontFamily: 'monospace',
                fontSize: '12px',
                wordBreak: 'break-all',
              }}
            >
              {item.original}
            </div>
          </div>

          {/* Plain systemd unit - simple input */}
          {!isOcf && (
            <div>
              <span className="text-sm font-semibold">
                Systemd Unit Configuration
              </span>
              {renderPlainUnitField()}
            </div>
          )}

          {/* OCF Agent with metadata */}
          {isOcf && metadata && (
            <div className="mb-4">
              <span className="text-sm font-semibold">{metadata.name}</span>
              {metadata.shortdesc && (
                <div className="mt-1 text-xs text-muted-foreground">
                  {metadata.shortdesc}
                </div>
              )}
            </div>
          )}

          {/* Form fields with metadata */}
          {isOcf && metadata && (
            <div className="mb-4">
              <div className="mb-3 flex items-center justify-between">
                <span className="text-sm font-semibold">Parameters</span>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => onAddParam?.(stableKey)}
                >
                  <Plus className="mr-1 h-4 w-4" />
                  Add Parameter
                </Button>
              </div>

              {/* Only show parameters that exist in TOML or were added by user */}
              {(ocfAgent?.params || []).map((paramEntry) => {
                const param = metadata.parameters.find(
                  (p) => p.name === paramEntry.key,
                );
                if (!param) return null;

                return (
                  <div
                    key={paramEntry.key}
                    className="relative"
                    style={{ paddingRight: '40px' }}
                  >
                    {renderFormField(param)}
                    <Button
                      variant="ghost"
                      size="icon"
                      className="absolute right-0 h-7 w-7 text-destructive"
                      style={{
                        top: param.type === 'boolean' ? '0' : '24px',
                      }}
                      onClick={() => onRemoveParam(stableKey, paramEntry.key)}
                    >
                      <MinusCircle className="h-4 w-4" />
                    </Button>
                  </div>
                );
              })}

              {/* Show manually added parameters */}
              {Array.from(addedParams.get(stableKey) || []).map((paramName) => {
                const param = metadata.parameters.find(
                  (p) => p.name === paramName,
                );
                if (!param || hasParam(ocfAgent?.params, paramName))
                  return null;

                return (
                  <div
                    key={paramName}
                    className="relative"
                    style={{ paddingRight: '40px' }}
                  >
                    {renderFormField(param)}
                    <Button
                      variant="ghost"
                      size="icon"
                      className="absolute right-0 h-7 w-7 text-destructive"
                      style={{
                        top: param.type === 'boolean' ? '0' : '24px',
                      }}
                      onClick={() => onRemoveParam(stableKey, paramName)}
                    >
                      <MinusCircle className="h-4 w-4" />
                    </Button>
                  </div>
                );
              })}
            </div>
          )}

          {/* OCF Agent without metadata */}
          {isOcf && !metadata && ocfAgent && (
            <div>
              <span className="text-sm font-semibold">Parsed Parameters:</span>
              <div
                style={{
                  marginTop: '8px',
                  padding: '12px',
                  background: currentTheme === 'dark' ? '#0f172a' : '#f1f5f9',
                  borderRadius: '4px',
                }}
              >
                <pre style={{ margin: 0, fontSize: '12px' }}>
                  {JSON.stringify(ocfAgent.params, null, 2)}
                </pre>
              </div>
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}
