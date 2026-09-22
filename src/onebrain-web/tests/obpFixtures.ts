import parent from '../../test-vectors/vnext/obp-product-orchestration-v1.json';
import transport from '../../test-vectors/vnext/obp-local-api-v1.json';
import { createObpClient } from '../src/api/obp';
import type { Fields } from '../src/api/obpContract';
export const id = '1'.repeat(64), peer = '2'.repeat(64), key = '3'.repeat(64);
export const session = { process_generation: '4'.repeat(64), dataset_generation: '5'.repeat(64) };
export const status = parent.fixtures.find(f => f.name === 'safe_default')!.value;
export const meta = { lifecycle: 'disabled', coverage: 'partial', limitations: [], continuation: null };
export const origin = 'http://127.0.0.1:4280';
export const token = 'PRIVATE_BEARER', management = 'obm1.' + 'x'.repeat(43);
export function response(data: unknown) { return new Response(JSON.stringify({ ok: true, profile: 'VNEXT_PRODUCT_INTEGRATION_PROFILE_V1', data, meta }), { headers: { 'content-type': 'application/json' } }); }
export function failure(reason: string) {
  const rule = transport.errors.find(e => e.reason === reason)!;
  return new Response(JSON.stringify({ ok: false, profile: 'VNEXT_PRODUCT_INTEGRATION_PROFILE_V1', meta, error: { code: rule.code, retryable: rule.retryable, obp: { reason, outcome: rule.outcome, reconcile_before_retry: rule.reconcile_before_retry } } }), { status: rule.http, headers: { 'content-type': 'application/json' } });
}
export function fixture(options: { lost?: boolean; unavailable?: boolean; expiredPage?: boolean } = {}) {
  const calls: { path: string; body: any; init: RequestInit }[] = [];
  let currentSession = { ...session };
  let reconcileState = 'completed';
  let command: any;
  const page = { items: [], snapshot_frontier: id, coverage: 'partial', limitations: [], claims_global_completion: false, authorizes_reward: false };
  const fetcher: typeof fetch = async (url, init = {}) => {
    const path = String(url).split('/api/vnext/obp/')[1]; const body = init.body ? JSON.parse(String(init.body)) : undefined;
    calls.push({ path, body, init });
    if (path === 'status') return response(options.unavailable ? { available: false, compiled: false } : { available: true, session: currentSession, status });
    if (path === 'query') {
      if (body.payload.continuation && options.expiredPage) return failure('snapshot_expired');
      if (body.operation === 'source_list') return response({ ...page, continuation: 'obc1.test', items: [{ source_id: id, kind: 'manual_invitation', state: 'unavailable', admitted_records: 0, limitations: ['host_binding_unavailable'] }] });
      if (body.operation === 'reservation_list') return response(page);
      if (body.operation === 'route_status') return response({ route_id: body.payload.route_id, expected_peer: peer, state: 'path_limited', generation: 1, coverage: 'partial', limitations: ['PathLimited'], claims_global_completion: false, authorizes_reward: false, failure: 'PathLimited' });
      if (body.operation === 'intent_status') return response({ intent_id: body.payload.intent_id, expected_peer: peer, state: 'pending', transport_attempts: 2, validation_retries: 1, coverage: 'partial', limitations: [], claims_global_completion: false, authorizes_reward: false });
    }
    if (path === 'commands') {
      command = body;
      if (options.lost) throw new TypeError('PRIVATE transport details must not display');
      return response({ idempotency_key: body.payload.idempotency_key, operation: body.operation, state: 'failed_no_effect', reconcile_before_retry: false, failure: transport.errors.find(e => e.reason === 'dependency_unavailable') });
    }
    if (path === 'reconcile') {
      if (reconcileState === 'local_not_found') return failure('local_not_found');
      return response({ idempotency_key: body.idempotency_key, operation: command?.operation || 'refresh', state: reconcileState, reconcile_before_retry: ['admitted', 'reconcile_required'].includes(reconcileState) });
    }
    throw new Error('Unexpected test route');
  };
  return { client: createObpClient(async () => ({ baseUrl: origin, token }), fetcher), calls, fetcher,
    restart() { currentSession = { ...currentSession, process_generation: '6'.repeat(64) }; },
    replaceDataset() { currentSession = { ...currentSession, dataset_generation: '7'.repeat(64) }; },
    setReconcile(state: string) { reconcileState = state; }, page,
  };
}
export const record = { origin, session, operation: 'refresh', payload: { idempotency_key: key, expected_generation: 1 } as Fields };
