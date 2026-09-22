import { fields, hexId, object, operations, parsePrivateJson, requireValue, validate } from './obpContract';
import type { Fields } from './obpContract';
import transportInventory from '../../../test-vectors/vnext/obp-local-api-v1.json';
export type Session = { process_generation: string; dataset_generation: string };
export type Meta = { lifecycle: string; coverage: string; limitations: string[]; continuation: null | string };
export type Status = Fields & { generation: number; limitations: string[] };
export type Snapshot = { available: false; compiled: boolean } | { available: true; session: Session; status: Status };
export type Result<T> = { data: T; meta: Meta };
export type Page = Fields & { items: Fields[]; continuation?: string; snapshot_frontier: string };
export type Recovery = { origin: string; session: Session; operation: string; payload: Fields };
export class ObpError extends Error {
  constructor(public reason: string, public uncertain = false) { super(reason); this.name = 'ObpError'; }
}
export function localOrigin(base: string): string {
  const u = new URL(base);
  requireValue(['http:', 'https:'].includes(u.protocol) && ['127.0.0.1', '[::1]'].includes(u.hostname) && !u.username && !u.password && !u.search && !u.hash && u.pathname === '/');
  return u.origin;
}
export function sessionValue(v: unknown): Session {
  const s = fields(v, ['process_generation', 'dataset_generation']);
  requireValue(hexId(s.process_generation) && hexId(s.dataset_generation)); return s as Session;
}
function metaValue(v: unknown): Meta {
  const m = fields(v, ['lifecycle', 'coverage', 'limitations', 'continuation']);
  validate('Lifecycle', m.lifecycle); validate('Coverage', m.coverage); validate('Limitations', m.limitations);
  if (m.continuation !== null) validate('Continuation', m.continuation); return m as Meta;
}
const errorRules = transportInventory.errors;
export function operationValue(v: unknown, operation: string, payload: Fields, metadata = false): Fields {
  const o = fields(v, ['idempotency_key', 'operation', 'state', 'reconcile_before_retry'], metadata ? [] : ['result', 'failure']);
  requireValue(o.idempotency_key === payload.idempotency_key && o.operation === operation);
  requireValue(['admitted', 'completed', 'failed_no_effect', 'reconcile_required'].includes(String(o.state)));
  const uncertain = o.state === 'admitted' || o.state === 'reconcile_required';
  requireValue(o.reconcile_before_retry === uncertain);
  if (metadata) return o;
  if (o.state === 'completed') {
    requireValue('result' in o && !('failure' in o));
    validate(operations.find(op => op.name === operation)!.response, o.result); matchTarget(operation, payload, object(o.result));
  } else if (o.state === 'failed_no_effect') {
    requireValue(!('result' in o)); const f = fields(o.failure, ['code', 'reason', 'retryable', 'outcome', 'reconcile_before_retry', 'http']);
    const rule = errorRules.find(r => r.reason === f.reason);
    requireValue(rule && rule.outcome === 'not_admitted' && Object.entries(rule).every(([k, v]) => f[k] === v));
  } else requireValue(!('result' in o) && !('failure' in o));
  return o;
}
function matchTarget(op: string, request: Fields, result: Fields) {
  const key = op === 'route_request' ? 'expected_peer' : op === 'route_status' ? 'route_id' : op.startsWith('intent_') ? 'intent_id' : op === 'source_set_enabled' ? 'source_id' : null;
  if (key) requireValue(request[key] === result[key]);
}
export function readRecovery(text: string): Recovery {
  const r = fields(parsePrivateJson(text), ['origin', 'session', 'operation', 'payload']);
  requireValue(typeof r.origin === 'string' && localOrigin(r.origin) === r.origin); sessionValue(r.session);
  const op = operations.find(o => o.name === r.operation && o.effect !== 'none'); requireValue(op); validate(op.request, r.payload);
  return r as Recovery;
}
export function createObpClient(connection: () => Promise<{ baseUrl: string; token: string }>, transport: typeof fetch = fetch) {
  let clientSession = '';
  async function request<T>(path: string, body?: unknown, management?: string): Promise<Result<T>> {
    const { baseUrl, token } = await connection(); const origin = localOrigin(baseUrl);
    const controller = new AbortController(); const timer = setTimeout(() => controller.abort(), 30000);
    try {
      const headers: Record<string, string> = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' };
      if (management) headers['X-OneBrain-OBP-Management'] = management;
      if (clientSession) headers['X-OneBrain-VNext-Client-Session'] = clientSession;
      const response = await transport(`${origin}/api/vnext/obp/${path}`, { method: body ? 'POST' : 'GET', headers, body: body ? JSON.stringify(body) : undefined, cache: 'no-store', redirect: 'error', credentials: 'omit', referrerPolicy: 'no-referrer', signal: controller.signal });
      if (response.status === 401 || response.status === 403) throw new ObpError('Host authentication or scoped grant unavailable');
      requireValue(response.headers.get('content-type')?.toLowerCase().startsWith('application/json'));
      const reader = response.body?.getReader(); requireValue(reader); const chunks: Uint8Array[] = []; let length = 0;
      while (true) { const { done, value } = await reader.read(); if (done) break; length += value.byteLength; if (length > 1048576) { await reader.cancel(); throw new Error('Response bound'); } chunks.push(value); }
      const bytes = new Uint8Array(length); let offset = 0; for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
      const envelope = object(parsePrivateJson(new TextDecoder('utf-8', { fatal: true }).decode(bytes)));
      requireValue(envelope.profile === 'VNEXT_PRODUCT_INTEGRATION_PROFILE_V1');
      const meta = metaValue(envelope.meta);
      if (envelope.ok === false) {
        const error = object(envelope.error);
        const detail = fields(error.obp, ['reason', 'outcome', 'reconcile_before_retry'], ['idempotency_key']);
        const rule = errorRules.find(r => r.reason === detail.reason);
        requireValue(rule && rule.http === response.status && rule.code === error.code && rule.retryable === error.retryable && rule.outcome === detail.outcome && rule.reconcile_before_retry === detail.reconcile_before_retry);
        throw new ObpError(String(detail.reason), rule.reconcile_before_retry);
      }
      requireValue(response.ok && envelope.ok === true); return { data: envelope.data as T, meta };
    } catch (e) { if (e instanceof ObpError) throw e; throw new ObpError('Transport or invalid response: read state and reconcile before retry', true); }
    finally { clearTimeout(timer); }
  }
  const client = {
    async origin() { return localOrigin((await connection()).baseUrl); },
    async status() {
      const r = await request<Snapshot>('status');
      try {
        if (r.data.available === false) { fields(r.data, ['available', 'compiled']); requireValue(typeof r.data.compiled === 'boolean'); }
        else { fields(r.data, ['available', 'session', 'status']); requireValue(r.data.available === true); sessionValue(r.data.session); validate('ObpStatusV1', r.data.status); }
      } catch { throw new ObpError('Invalid status projection', true); }
      return r;
    },
    async query<T = Fields>(session: Session, operation: string, payload: Fields): Promise<Result<T>> {
      sessionValue(session); const op = operations.find(o => o.name === operation && o.name !== 'status' && o.effect === 'none'); requireValue(op); validate(op.request, payload);
      const r = await request<T>('query', { session, operation, payload });
      validate(op.response, r.data); matchTarget(operation, payload, object(r.data)); return r;
    },
    async command(record: Recovery, management?: string) {
      requireValue(await client.origin() === record.origin); readRecovery(JSON.stringify(record));
      const op = operations.find(o => o.name === record.operation)!;
      if (op.access === 'host_management') requireValue(management && /^obm1\.[A-Za-z0-9_-]{43}$/.test(management));
      const r = await request<Fields>('commands', { session: record.session, operation: record.operation, payload: record.payload }, op.access === 'host_management' ? management : undefined);
      try { operationValue(r.data, record.operation, record.payload); } catch { throw new ObpError('Invalid command outcome; reconcile required', true); } return r;
    },
    async reconcile(record: Recovery) {
      requireValue(await client.origin() === record.origin);
      const fresh = await client.status();
      if (!fresh.data.available) throw new ObpError('Shared OBP service unavailable');
      if (fresh.data.session.dataset_generation !== record.session.dataset_generation) throw new ObpError('Dataset changed; original outcome remains unknown');
      const r = await request<Fields>('reconcile', { session: fresh.data.session, idempotency_key: record.payload.idempotency_key });
      operationValue(r.data, record.operation, record.payload, true); return r;
    },
    async subscribe(session: Session, onHint: () => void, onGap: () => void, socketFactory: (url: string) => WebSocket = url => new WebSocket(url)) {
      sessionValue(session);
      const r = await request<Fields>('ws/tickets', { session, subscriptions: ['network'] });
      fields(r.data, ['ticket', 'client_session', 'expires_at', 'session_expires_at', 'subscriptions', 'limitations']);
      requireValue(JSON.stringify(r.data.subscriptions) === '["network"]');
      validate('Count', r.data.expires_at); validate('Count', r.data.session_expires_at); validate('Limitations', r.data.limitations);
      requireValue(/^obw1\.[A-Za-z0-9_-]{43}$/.test(String(r.data.ticket)) && /^obw1\.[A-Za-z0-9_-]{43}$/.test(String(r.data.client_session)) && r.data.ticket !== r.data.client_session);
      const ws = socketFactory(`${(await client.origin()).replace(/^http/, 'ws')}/api/vnext/obp/ws?ticket=${encodeURIComponent(String(r.data.ticket))}`);
      let sequence = 0, closed = false;
      const stop = () => { if (closed) return; closed = true; clientSession = ''; ws.onclose = null; ws.onmessage = null; ws.onerror = null; ws.close(); };
      const gap = () => { if (closed) return; stop(); onGap(); };
      ws.onclose = gap; ws.onerror = gap;
      ws.onmessage = event => {
        try {
          requireValue(typeof event.data === 'string' && event.data.length <= 16384);
          const e = fields(parsePrivateJson(event.data), ['profile', 'event_type', 'sequence', 'timestamp', 'lifecycle', 'coverage', 'limitations', 'data']);
          requireValue(e.profile === 'OBP_PRIVATE_WEBSOCKET_PROFILE_V1' && e.sequence === sequence + 1);
          validate('Lifecycle', e.lifecycle); validate('Coverage', e.coverage); validate('Limitations', e.limitations); validate('Count', e.timestamp);
          if (sequence === 0) {
            requireValue(e.event_type === 'subscription_ready'); const d = fields(e.data, ['subscriptions', 'session_expires_at']);
            requireValue(JSON.stringify(d.subscriptions) === '["network"]'); validate('Count', d.session_expires_at); clientSession = String(r.data.client_session);
          } else {
            requireValue(e.event_type === 'network_state');
            const d = fields(e.data, ['compiled', 'requested', 'active', 'kill_switch', 'signer_ready', 'source_count', 'usable_reservations', 'authenticated_routes', 'pending_intents', 'advertisement_state', 'claims_global_completion', 'authorizes_reward']);
            validate('ObpStatusV1', { ...d, generation: 1, lifecycle: e.lifecycle, coverage: e.coverage, limitations: e.limitations });
            onHint();
          }
          sequence++;
        } catch { gap(); }
      };
      return stop;
    },
  };
  return client;
}
export type ObpClient = ReturnType<typeof createObpClient>;
