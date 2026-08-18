#!/usr/bin/env node
// compare-ra.mjs — diff my JS static-parser output against ra-params (authoritative,
// produced by executing each installed agent's meta-data).
//
// Usage: node compare-ra.mjs <js.json> <ra-params all_agents.json> [provider]
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const [jsPath, rpPath, provider = 'heartbeat'] = process.argv.slice(2);
const js = JSON.parse(readFileSync(jsPath, 'utf8'));
// rpPath may be a combined JSON array OR a directory of per-agent JSON files.
let rpRaw;
if (statSync(rpPath).isDirectory()) {
  rpRaw = readdirSync(rpPath)
    .filter((f) => f.endsWith('.json'))
    // ra-params converts backup leftovers too (e.g. ganesha-nfs.v01.bak), and they
    // declare the same agent name — drop them so a stale copy can't win the key.
    .filter((f) => !/(\.bak|\.orig|\.rpmsave|\.rpmnew|~)\.json$/i.test(f))
    .map((f) => JSON.parse(readFileSync(join(rpPath, f), 'utf8')))
    .map((ra) => ({ ...ra, provider: ra.provider || provider }));
} else {
  rpRaw = JSON.parse(readFileSync(rpPath, 'utf8'));
}

const norm = (s) => (s || '').replace(/\s+/g, ' ').trim();

// Flatten ra-params nested model -> same shape my JS emits (mirrors toml_parse::ResourceAgent::from)
function flattenRp(ra) {
  const params = (ra.parameters?.parameter || []).map((p) => ({
    name: p['@name'] || '',
    unique: p['@unique'] === '1',
    required: p['@required'] === '1',
    shortdesc: p.shortdesc?.$value || '',
    longdesc: p.longdesc?.$value || '',
    type: p.content?.['@type'] || '',
    default: p.content?.['@default'] || '',
  }));
  const actions = (ra.actions?.action || []).map((a) => ({
    name: a['@name'] || '', timeout: a['@timeout'] || '', interval: a['@interval'] || '', depth: a['@depth'] || '',
  }));
  return {
    name: ra['@name'] || '',
    version: ra['@version'] || ra.version || '0.0',
    shortdesc: ra.shortdesc?.$value || '',
    longdesc: ra.longdesc?.$value || '',
    parameters: params, actions,
    provider: ra.provider || '',
  };
}

const jsAgents = new Map(js.providers[provider].map((a) => [a.name, a]));
const rpAgents = new Map(
  rpRaw.map(flattenRp).filter((a) => a.provider === provider || a.provider === '').map((a) => [a.name, a])
);

const onlyRp = [...rpAgents.keys()].filter((n) => !jsAgents.has(n)).sort();
const onlyJs = [...jsAgents.keys()].filter((n) => !rpAgents.has(n)).sort();
const common = [...jsAgents.keys()].filter((n) => rpAgents.has(n)).sort();

let identical = 0;
const structuralDiffs = []; // param set / type / required / unique / action / desc / version
const defaultOnlyDiffs = []; // only content.default differs

for (const name of common) {
  const a = jsAgents.get(name), b = rpAgents.get(name);
  const issues = [];
  let onlyDefault = true;

  if (a.version !== b.version) { issues.push(`version ${a.version}≠${b.version}`); onlyDefault = false; }
  if (norm(a.shortdesc) !== norm(b.shortdesc)) { issues.push('shortdesc'); onlyDefault = false; }
  if (norm(a.longdesc) !== norm(b.longdesc)) { issues.push('longdesc'); onlyDefault = false; }

  const ap = new Map(a.parameters.map((p) => [p.name, p]));
  const bp = new Map(b.parameters.map((p) => [p.name, p]));
  const missing = [...bp.keys()].filter((k) => !ap.has(k));
  const extra = [...ap.keys()].filter((k) => !bp.has(k));
  if (missing.length) { issues.push(`params missing: ${missing.join(',')}`); onlyDefault = false; }
  if (extra.length) { issues.push(`params extra: ${extra.join(',')}`); onlyDefault = false; }

  const defaultMismatches = [];
  for (const k of [...ap.keys()].filter((k) => bp.has(k))) {
    const x = ap.get(k), y = bp.get(k);
    if (x.type !== y.type) { issues.push(`${k}.type ${x.type}≠${y.type}`); onlyDefault = false; }
    if (x.required !== y.required) { issues.push(`${k}.required ${x.required}≠${y.required}`); onlyDefault = false; }
    if (x.unique !== y.unique) { issues.push(`${k}.unique ${x.unique}≠${y.unique}`); onlyDefault = false; }
    if (x.default !== y.default) defaultMismatches.push(`${k}: ${JSON.stringify(x.default)}≠${JSON.stringify(y.default)}`);
  }

  // actions
  const aa = JSON.stringify(a.actions), ba = JSON.stringify(b.actions);
  if (aa !== ba) {
    // compare as sets by name to reduce ordering noise
    const an = new Map(a.actions.map((x) => [x.name, x])), bn = new Map(b.actions.map((x) => [x.name, x]));
    const amiss = [...bn.keys()].filter((k) => !an.has(k)), aextra = [...an.keys()].filter((k) => !bn.has(k));
    const aval = [...an.keys()].filter((k) => bn.has(k)).filter((k) => JSON.stringify(an.get(k)) !== JSON.stringify(bn.get(k)));
    if (amiss.length || aextra.length || aval.length) {
      issues.push(`actions diff (miss:${amiss.join(',')||'-'} extra:${aextra.join(',')||'-'} val:${aval.join(',')||'-'})`);
      onlyDefault = false;
    }
  }

  if (issues.length === 0 && defaultMismatches.length === 0) { identical++; continue; }
  if (issues.length === 0 && defaultMismatches.length > 0) {
    defaultOnlyDiffs.push({ name, defaults: defaultMismatches });
  } else {
    structuralDiffs.push({ name, issues, defaults: defaultMismatches });
  }
}

console.log(`Provider: ${provider}`);
console.log(`ra-params agents: ${rpAgents.size} | my JS agents: ${jsAgents.size} | common: ${common.length}`);
console.log(`\nCoverage gap:`);
console.log(`  Only ra-params (JS missed): ${onlyRp.length} -> ${onlyRp.join(', ') || '-'}`);
console.log(`  Only JS (ra-params missed): ${onlyJs.length} -> ${onlyJs.join(', ') || '-'}`);
console.log(`\nOf ${common.length} common agents:`);
console.log(`  ✅ byte-identical (all fields incl. defaults): ${identical}`);
console.log(`  🟡 default-value-only diffs (expected static-parse limit): ${defaultOnlyDiffs.length}`);
console.log(`  🔴 STRUCTURAL diffs (parser correctness): ${structuralDiffs.length}`);

if (structuralDiffs.length) {
  console.log(`\n=== 🔴 STRUCTURAL DIFFS (these matter) ===`);
  for (const d of structuralDiffs) console.log(`  ${d.name}: ${d.issues.join(' | ')}`);
}

console.log(`\n=== 🟡 default-only diffs (sample up to 12 agents) ===`);
for (const d of defaultOnlyDiffs.slice(0, 12)) {
  console.log(`  ${d.name}:`);
  for (const m of d.defaults.slice(0, 4)) console.log(`      ${m}`);
  if (d.defaults.length > 4) console.log(`      ... +${d.defaults.length - 4} more`);
}
console.log(`\nTotal default-value mismatches across all common agents: ${defaultOnlyDiffs.reduce((s, d) => s + d.defaults.length, 0) + structuralDiffs.reduce((s, d) => s + d.defaults.length, 0)}`);
