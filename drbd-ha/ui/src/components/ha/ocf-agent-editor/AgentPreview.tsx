import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Spinner } from '@/components/ui/spinner';
import type {
  OcfAgentWithMetadata,
  ParamEntry,
  ParsedOcfAgent,
} from '@/api/ha-profiles';

// Helper function to generate OCF string from agent data
function generateOcfString(
  agent: ParsedOcfAgent,
  params?: ParamEntry[],
): string {
  const { provider, agent_type, instance_name } = agent;
  const finalParams = params || agent.params;

  // Build key=value pairs - order is preserved from the array
  const paramStr = finalParams
    .map(({ key, value }) => {
      if (value === undefined || value === null) return '';
      // Quote values if they contain spaces or special characters
      if (
        String(value).includes(' ') ||
        String(value).includes(',') ||
        String(value) === ''
      ) {
        return `${key}='${value}'`;
      }
      return `${key}=${value}`;
    })
    .filter(Boolean)
    .join(' ');

  return `ocf:${provider}:${agent_type} ${instance_name}${paramStr ? ` ${paramStr}` : ''}`;
}

interface AgentPreviewProps {
  parsedAgents: OcfAgentWithMetadata[];
  loading: boolean;
  currentTheme: string;
}

export function AgentPreview({
  parsedAgents,
  loading,
  currentTheme,
}: AgentPreviewProps) {
  // Generate OCF string from agent data
  const generateAgentString = (itemWithMeta: OcfAgentWithMetadata): string => {
    if (itemWithMeta.item.is_ocf && itemWithMeta.item.ocf_agent) {
      // OCF agent - use generateOcfString
      const agent = itemWithMeta.item.ocf_agent;
      const params = agent.params || [];
      const result = generateOcfString(agent, params);
      return `    "${result}"`;
    } else {
      // Plain systemd unit - use item.original directly
      return `    "${itemWithMeta.item.original}"`;
    }
  };

  // Generate full TOML start array preview
  const generateTomlPreview = (): string => {
    const agentStrings = parsedAgents.map((agentWithMeta) => {
      return generateAgentString(agentWithMeta);
    });

    return `start = [\n${agentStrings.join(',\n')}\n  ]`;
  };

  return (
    <div
      style={{
        flex: 0.6,
        overflow: 'hidden',
        minWidth: 0,
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      <Card
        className="flex h-full flex-col border-0 shadow-none"
      >
        <CardHeader className="px-4 py-3">
          <CardTitle className="text-sm font-semibold">
            Live Preview (TOML)
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-1 flex-col overflow-hidden p-4">
          {loading ? (
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <Spinner className="h-4 w-4" />
              Generating preview...
            </div>
          ) : (
            <div
              style={{
                background: currentTheme === 'dark' ? '#0f172a' : '#f1f5f9',
                borderRadius: '8px',
                padding: '16px',
                fontFamily: 'monospace',
                fontSize: '13px',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word',
                border: `1px solid ${currentTheme === 'dark' ? '#334155' : '#e2e8f0'}`,
                overflow: 'auto',
                flex: 1,
                maxHeight: '100%',
              }}
            >
              {generateTomlPreview()}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
