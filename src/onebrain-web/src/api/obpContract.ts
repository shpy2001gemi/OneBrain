import inventory from '../../../test-vectors/vnext/obp-product-orchestration-v1.json';

export type Fields = Record<string, unknown>;
export const operations = inventory.operations;
export const sourceKinds = inventory.types.SourceKind.values;
export const hexId = (v: unknown): v is string => typeof v === 'string' && /^[0-9a-f]{64}$/.test(v);
export function requireValue(ok: unknown): asserts ok { if (!ok) throw new Error('Invalid OBP projection'); }
export function object(v: unknown): Fields {
  requireValue(v !== null && typeof v === 'object' && !Array.isArray(v));
  return v as Fields;
}
export function fields(v: unknown, required: string[], optional: string[] = []): Fields {
  const o = object(v);
  requireValue(required.every(k => Object.hasOwn(o, k)) && Object.keys(o).every(k => [...required, ...optional].includes(k)));
  return o;
}
type Rule = { kind: string; bytes?: number; min?: number; max?: number; max_bytes?: number; max_items?: number; items?: string; values?: unknown[]; value?: unknown };
type Dto = { required: Record<string, string>; optional: Record<string, string> };
const types: Record<string, Rule> = inventory.types;
const dtos: Record<string, Dto> = inventory.dtos;
export function validate(name: string, value: unknown): void {
  const dto = dtos[name];
  if (dto) {
    const v = fields(value, Object.keys(dto.required), Object.keys(dto.optional));
    for (const [key, type] of Object.entries({ ...dto.required, ...dto.optional })) if (Object.hasOwn(v, key)) validate(type, v[key]);
    if (name === 'ObpStatusV1') {
      if (v.active) requireValue(v.compiled && v.requested && v.signer_ready && !v.kill_switch && ['active', 'degraded'].includes(String(v.lifecycle)));
      if (v.lifecycle === 'active') requireValue(v.active);
      if (v.kill_switch || !v.requested || !v.compiled) requireValue(!v.active && v.lifecycle === 'disabled');
      requireValue(Number(v.usable_reservations) <= 3);
    }
    if (name === 'ObpRouteV1') {
      const auth = ['authenticated_peer', 'path_kind', 'route_receipt_digest'];
      if (v.state === 'connected') requireValue(auth.every(k => k in v) && v.authenticated_peer === v.expected_peer && !('failure' in v));
      else requireValue(auth.every(k => !(k in v)));
      if (v.state === 'path_limited') requireValue('failure' in v && (v.limitations as unknown[]).length);
    }
    if (name === 'ObpIntentV1') {
      if (v.state === 'acknowledged') requireValue(Number(v.acknowledged_sequence) > 0 && 'checkpoint_digest' in v);
      else requireValue(!('acknowledged_sequence' in v) && !('checkpoint_digest' in v));
    }
    return;
  }
  const rule = types[name];
  requireValue(rule);
  switch (rule.kind) {
    case 'hex': requireValue(hexId(value)); break;
    case 'boolean': requireValue(typeof value === 'boolean'); break;
    case 'literal': requireValue(value === rule.value); break;
    case 'integer': requireValue(Number.isSafeInteger(value) && Number(value) >= rule.min! && Number(value) <= rule.max!); break;
    case 'enum': requireValue(rule.values!.includes(value)); break;
    case 'string':
      requireValue(typeof value === 'string' && new TextEncoder().encode(value).length <= rule.max_bytes!);
      if (name === 'Continuation') requireValue(value.startsWith('obc1'));
      break;
    case 'array':
      requireValue(Array.isArray(value) && value.length <= rule.max_items!);
      value.forEach(v => validate(rule.items!, v)); break;
    default: throw new Error('Invalid OBP type');
  }
}

// Reject duplicate keys, excessive nesting and unsafe integers before projection.
export function parsePrivateJson(text: string): unknown {
  requireValue(new TextEncoder().encode(text).length <= 1048576);
  let i = 0;
  function whitespace() { while (/[ \t\r\n]/.test(text[i] || '') && i < text.length) i++; }
  function value(depth: number): unknown {
    requireValue(depth <= 16); whitespace();
    if (text[i] === '{') {
      i++; whitespace(); const result: Fields = Object.create(null);
      if (text[i] === '}') { i++; return result; }
      while (true) {
        whitespace(); requireValue(text[i] === '"'); const key = value(depth + 1) as string;
        requireValue(!Object.hasOwn(result, key)); whitespace(); requireValue(text[i++] === ':');
        result[key] = value(depth + 1); whitespace(); const end = text[i++];
        if (end === '}') return result; requireValue(end === ',');
      }
    }
    if (text[i] === '[') {
      i++; whitespace(); const result: unknown[] = [];
      if (text[i] === ']') { i++; return result; }
      while (true) { result.push(value(depth + 1)); whitespace(); const end = text[i++]; if (end === ']') return result; requireValue(end === ','); }
    }
    // JSON forbids literal control bytes inside strings.
    // eslint-disable-next-line no-control-regex
    const match = /^("(?:[^"\\\x00-\x1f]|\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4}))*"|true|false|null|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)/.exec(text.slice(i));
    requireValue(match); i += match[0].length; const result: unknown = JSON.parse(match[0]);
    if (typeof result === 'number') requireValue(Number.isSafeInteger(result));
    return result;
  }
  const result = value(0); whitespace(); requireValue(i === text.length); return result;
}
