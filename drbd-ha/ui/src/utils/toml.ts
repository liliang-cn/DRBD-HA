/**
 * Parse an OCF agent string into structured data
 * Format: "ocf:provider:agent key=value key=value"
 */
export interface OcfAgentString {
  prefix: string; // e.g., "ocf:heartbeat:portblock"
  provider: string; // e.g., "heartbeat"
  agent: string; // e.g., "portblock"
  params: Record<string, string>; // key-value pairs
  original: string; // original string for reference
}

export function parseOcfAgentString(input: string): OcfAgentString | null {
  const trimmed = input.trim();

  // Check if it starts with "ocf:"
  if (!trimmed.startsWith('ocf:')) {
    return null;
  }

  // Find the end of the prefix (first space after ocf:provider:agent)
  const firstSpaceIndex = trimmed.indexOf(' ');
  if (firstSpaceIndex === -1) {
    // No parameters, just the prefix
    const prefix = trimmed;
    const parts = prefix.split(':');
    if (parts.length < 3) return null;

    return {
      prefix,
      provider: parts[1],
      agent: parts[2],
      params: {},
      original: trimmed,
    };
  }

  const prefix = trimmed.substring(0, firstSpaceIndex);
  const paramsString = trimmed.substring(firstSpaceIndex + 1);

  const parts = prefix.split(':');
  if (parts.length < 3) return null;

  // Parse key=value pairs
  const params: Record<string, string> = {};
  const paramParts = paramsString.split(/\s+/).filter(Boolean);

  for (const part of paramParts) {
    const equalIndex = part.indexOf('=');
    if (equalIndex > 0) {
      const key = part.substring(0, equalIndex);
      const value = part.substring(equalIndex + 1);
      params[key] = value;
    }
  }

  return {
    prefix,
    provider: parts[1],
    agent: parts[2],
    params,
    original: trimmed,
  };
}

/**
 * Convert OCF agent string back to string format
 */
export function serializeOcfAgentString(agent: OcfAgentString): string {
  const paramStr = Object.entries(agent.params)
    .map(([key, value]) => `${key}=${value}`)
    .join(' ');

  return paramStr ? `${agent.prefix} ${paramStr}` : agent.prefix;
}

/**
 * Parse TOML content and extract structured data
 * This is a simplified parser - for production use a proper TOML library
 */
export interface ParsedTomlSection {
  [key: string]: any;
}

export function parseTomlContent(content: string): ParsedTomlSection | null {
  try {
    // Use a simple TOML parser
    // For production, consider using a library like 'toml'
    const lines = content.split('\n');
    const result: ParsedTomlSection = {};
    let currentSection: string | null = null;

    for (const line of lines) {
      const trimmed = line.trim();

      // Skip comments and empty lines
      if (!trimmed || trimmed.startsWith('#')) {
        continue;
      }

      // Section header [section]
      if (trimmed.startsWith('[') && trimmed.endsWith(']')) {
        currentSection = trimmed.slice(1, -1);
        result[currentSection] = result[currentSection] || {};
        continue;
      }

      // Array section [[section]]
      if (trimmed.startsWith('[[') && trimmed.endsWith(']]')) {
        currentSection = trimmed.slice(2, -2);
        if (!Array.isArray(result[currentSection])) {
          result[currentSection] = [];
        }
        continue;
      }

      // Key = Value
      const equalIndex = trimmed.indexOf('=');
      if (equalIndex > 0) {
        const key = trimmed.substring(0, equalIndex).trim();
        const value = trimmed.substring(equalIndex + 1).trim();

        if (currentSection) {
          if (Array.isArray(result[currentSection])) {
            // Add to last item in array
            const lastItem =
              result[currentSection][result[currentSection].length - 1] || {};
            lastItem[key] = parseTomlValue(value);
            if (result[currentSection].length === 0) {
              result[currentSection].push(lastItem);
            }
          } else {
            result[currentSection][key] = parseTomlValue(value);
          }
        } else {
          result[key] = parseTomlValue(value);
        }
      }
    }

    return result;
  } catch (e) {
    console.error('Failed to parse TOML:', e);
    return null;
  }
}

/**
 * Extract the items of the `start = [ ... ]` array from arbitrary pasted TOML.
 * Handles multi-line arrays and both quote styles. Returns null if no start
 * array is found.
 */
export function extractStartArrayItems(content: string): string[] | null {
  // Locate `start` assignment (start of line, optional whitespace)
  const startMatch = content.match(/^[ \t]*start[ \t]*=[ \t]*\[/m);
  if (!startMatch || startMatch.index === undefined) return null;

  const openIndex = content.indexOf('[', startMatch.index);
  // Scan to the matching closing bracket, respecting quotes
  let depth = 0;
  let inQuote: '"' | "'" | null = null;
  let endIndex = -1;
  for (let i = openIndex; i < content.length; i++) {
    const ch = content[i];
    if (inQuote) {
      if (ch === inQuote) inQuote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      inQuote = ch;
    } else if (ch === '[') {
      depth++;
    } else if (ch === ']') {
      depth--;
      if (depth === 0) {
        endIndex = i;
        break;
      }
    }
  }
  if (endIndex === -1) return null;

  const inner = content.slice(openIndex + 1, endIndex);
  // Extract quoted strings ("..." or '...')
  const items: string[] = [];
  const itemRegex = /"((?:[^"\\]|\\.)*)"|'([^']*)'/g;
  let m: RegExpExecArray | null = itemRegex.exec(inner);
  while (m) {
    const raw = m[1] !== undefined ? m[1].replace(/\\(.)/g, '$1') : m[2];
    const trimmed = raw.trim();
    if (trimmed) items.push(trimmed);
    m = itemRegex.exec(inner);
  }
  return items;
}

/**
 * Parse a full OCF agent line into provider/agent/instance/params with order
 * preserved. Format: "ocf:provider:agent_type instance_name k=v k='v v'..."
 * Returns null for non-OCF lines.
 */
export interface ParsedOcfLine {
  provider: string;
  agent_type: string;
  instance_name: string;
  params: { key: string; value: string }[];
}

export function parseOcfLine(input: string): ParsedOcfLine | null {
  const m = input.trim().match(/^ocf:([^:]+):(\S+)\s+(\S+)(?:\s+(.*))?$/);
  if (!m) return null;
  const [, provider, agent_type, instance_name, paramsStr] = m;

  const params: { key: string; value: string }[] = [];
  if (paramsStr) {
    const paramRegex = /(\w+)=(?:'([^']*)'|"([^"]*)"|(\S+))/g;
    let pm: RegExpExecArray | null = paramRegex.exec(paramsStr);
    while (pm) {
      params.push({ key: pm[1], value: pm[2] ?? pm[3] ?? pm[4] ?? '' });
      pm = paramRegex.exec(paramsStr);
    }
  }
  return { provider, agent_type, instance_name, params };
}

/**
 * Parse a TOML value (string, number, boolean, array)
 */
function parseTomlValue(value: string): any {
  value = value.trim();

  // String literal
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1);
  }

  // Boolean
  if (value === 'true') return true;
  if (value === 'false') return false;

  // Number
  if (/^\d+(\.\d+)?$/.test(value)) {
    return parseFloat(value);
  }

  // Array
  if (value.startsWith('[') && value.endsWith(']')) {
    const inner = value.slice(1, -1);
    if (!inner.trim()) return [];
    return inner.split(',').map(parseTomlValue);
  }

  // Default: return as string
  return value;
}
