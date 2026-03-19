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
