/**
 * Helpers for reading drbd-reactor promoter TOML in the OCF agent editor.
 */

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
