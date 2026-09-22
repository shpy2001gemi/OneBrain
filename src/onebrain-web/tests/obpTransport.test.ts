import { describe, expect, it, vi } from 'vitest';
import parent from '../../test-vectors/vnext/obp-product-orchestration-v1.json';
import transport from '../../test-vectors/vnext/obp-local-api-v1.json';
import { fields, parsePrivateJson, validate } from '../src/api/obpContract';
import { createObpClient, localOrigin, operationValue, readRecovery } from '../src/api/obp';
import { failure, fixture, key, management, origin, peer, record, response, session, status, token } from './obpFixtures';

describe('OBP contract projection', () => {
  for (const f of parent.fixtures) it(`parent fixture: ${f.name}`, () => {
    if (f.valid) expect(() => validate(f.dto, f.value)).not.toThrow();
    else expect(() => validate(f.dto, f.value)).toThrow();
  });
  it('rejects duplicate keys, excessive depth, unsafe integers and trailing data', () => {
    for (const raw of ['{"a":1,"a":2}', '{"a":1,"\\u0061":2}', '9007199254740993', '1.1', '[1,]', '{}{}', '['.repeat(18) + '0' + ']'.repeat(18)]) expect(() => parsePrivateJson(raw)).toThrow();
    expect(parsePrivateJson('{"hello":[true,null,"escaped\\ntext",123]}')).toEqual({ hello: [true, null, 'escaped\ntext', 123] });
  });
  it('accepts only local origins and closed recovery records', () => {
    for (const u of ['http://example.org', 'http://localhost', 'http://127.0.0.1/path', 'http://secret@127.0.0.1', 'http://127.0.0.1?token=x']) expect(() => localOrigin(u)).toThrow();
    expect(localOrigin('http://[::1]:4280')).toBe('http://[::1]:4280');
    expect(readRecovery(JSON.stringify(record))).toEqual(record);
    expect(() => readRecovery(JSON.stringify({ ...record, management }))).toThrow();
  });
  it('does not accept false command or metadata outcomes', () => {
    const completed = { idempotency_key: key, operation: 'refresh', state: 'completed', reconcile_before_retry: false, result: status };
    expect(() => operationValue(completed, 'refresh', record.payload)).not.toThrow();
    expect(() => operationValue(completed, 'refresh', record.payload, true)).toThrow();
    expect(() => operationValue({ ...completed, result: undefined }, 'refresh', record.payload)).toThrow();
    expect(() => operationValue({ ...completed, idempotency_key: peer }, 'refresh', record.payload)).toThrow();
    expect(() => operationValue({ ...completed, state: 'admitted' }, 'refresh', record.payload)).toThrow();
    const failed = transport.fixtures.find(f => f.name === 'no_effect_command')!.value;
    expect(() => operationValue(failed, 'refresh', record.payload)).not.toThrow();
  });
});

describe('private transport and recovery', () => {
  it('maps all 13 operations through accepted routes without leaking credentials', async () => {
    const calls: any[] = [];
    const client = createObpClient(async () => ({ baseUrl: origin, token }), async (url, init) => {
      const body = init?.body && JSON.parse(String(init.body)); calls.push({ url, init, body });
      if (String(url).endsWith('/status')) return response({ available: true, session, status });
      const op = parent.operations.find(o => o.name === body.operation)!;
      if (op.effect !== 'none') return response({ idempotency_key: key, operation: op.name, state: 'admitted', reconcile_before_retry: true });
      const result: any = op.name.endsWith('_list')
        ? { items: [], snapshot_frontier: key, coverage: 'partial', limitations: [], claims_global_completion: false, authorizes_reward: false }
        : structuredClone(parent.fixtures.find(f => f.dto === op.response && f.valid)!.value);
      if (body.payload.route_id) result.route_id = body.payload.route_id;
      if (body.payload.intent_id) result.intent_id = body.payload.intent_id;
      return response(result);
    });
    const payloads: Record<string, any> = {
      configure: { outbound_first_requested: false, advertise_reachability: false }, source_admit: { input_ref: peer, kind: 'manual_invitation' }, source_set_enabled: { source_id: peer, enabled: false },
      refresh: {}, route_request: { expected_peer: peer }, intent_retry: { intent_id: peer }, network_kill: {}, network_reenable: {},
      source_list: { limit: 32 }, reservation_list: { limit: 32 }, route_status: { route_id: peer }, intent_status: { intent_id: peer },
    };
    await client.status();
    for (const op of parent.operations.filter(o => o.name !== 'status')) {
      if (op.effect === 'none') await client.query(session, op.name, payloads[op.name]);
      else await client.command({ ...record, operation: op.name, payload: { ...record.payload, ...payloads[op.name] } }, management);
    }
    expect(calls).toHaveLength(13);
    for (const call of calls) {
      expect(call.init.headers.Authorization).toBe(`Bearer ${token}`);
      expect(call.init.redirect).toBe('error'); expect(call.init.cache).toBe('no-store'); expect(call.init.credentials).toBe('omit');
      expect(String(call.url)).not.toContain(token); expect(JSON.stringify(call.body) || '').not.toContain(management);
      const op = parent.operations.find(o => o.name === call.body?.operation);
      expect(call.init.headers['X-OneBrain-OBP-Management']).toBe(op?.access === 'host_management' ? management : undefined);
    }
  });
  for (const rule of transport.errors) it(`preserves error policy: ${rule.reason}`, async () => {
    const client = createObpClient(async () => ({ baseUrl: origin, token }), async () => failure(rule.reason));
    await expect(client.command(record)).rejects.toMatchObject({ reason: rule.reason, uncertain: rule.reconcile_before_retry });
  });
  it('lost response performs one dispatch, then only fresh-status/reconcile across process restart', async () => {
    const f = fixture({ lost: true });
    await expect(f.client.command(record)).rejects.toMatchObject({ uncertain: true }); f.restart();
    await f.client.reconcile(record);
    expect(f.calls.map(c => c.path)).toEqual(['commands', 'status', 'reconcile']);
    expect(f.calls[2].body.session.process_generation).not.toBe(session.process_generation);
    expect(f.calls[2].body.idempotency_key).toBe(key);
    f.replaceDataset(); await expect(f.client.reconcile(record)).rejects.toThrow('Dataset changed');
    expect(f.calls.filter(c => c.path === 'commands')).toHaveLength(1);
    expect(f.calls.filter(c => c.path === 'reconcile')).toHaveLength(1);
  });
  it('rejects malformed, oversized and invalid successful responses as unknown without logging', async () => {
    const log = vi.spyOn(console, 'log'), error = vi.spyOn(console, 'error');
    for (const raw of ['not JSON', ' '.repeat(1048577), JSON.stringify({ ok: true, data: {} })]) {
      const client = createObpClient(async () => ({ baseUrl: origin, token }), async () => new Response(raw, { headers: { 'content-type': 'application/json' } }));
      await expect(client.command(record)).rejects.toMatchObject({ uncertain: true });
    }
    expect(log).not.toHaveBeenCalled(); expect(error).not.toHaveBeenCalled(); log.mockRestore(); error.mockRestore();
  });
  it('never returns a route for a different requested identity', async () => {
    const route: any = parent.fixtures.find(f => f.name === 'authenticated_tcp_fallback')!.value;
    const client = createObpClient(async () => ({ baseUrl: origin, token }), async () => response({ idempotency_key: key, operation: 'route_request', state: 'completed', reconcile_before_retry: false, result: route }));
    await expect(client.command({ ...record, operation: 'route_request', payload: { ...record.payload, expected_peer: '9'.repeat(64) } })).rejects.toMatchObject({ uncertain: true });
  });
});

describe('private WS hints', () => {
  for (const violation of ['private_field', 'wrong_profile', 'duplicate_sequence']) it(`rejects ${violation} without projecting it`, async () => {
    const client = createObpClient(async () => ({ baseUrl: origin, token }), async () => response({ ticket: 'obw1.' + 'a'.repeat(43), client_session: 'obw1.' + 'b'.repeat(43), expires_at: 30, session_expires_at: 900, subscriptions: ['network'], limitations: [] }));
    const socket = { onmessage: null as any, onclose: null as any, onerror: null as any, close: vi.fn() };
    const hint = vi.fn(), gap = vi.fn();
    await client.subscribe(session, hint, gap, () => socket as any);
    const ready = { profile: 'OBP_PRIVATE_WEBSOCKET_PROFILE_V1', event_type: 'subscription_ready', sequence: 1, timestamp: 1, lifecycle: 'disabled', coverage: 'partial', limitations: [], data: { subscriptions: ['network'], session_expires_at: 900 } };
    socket.onmessage({ data: JSON.stringify(ready) });
    const aggregate = Object.fromEntries(Object.entries(status).filter(([k]) => !['generation', 'lifecycle', 'coverage', 'limitations'].includes(k)));
    const event = { ...ready, event_type: 'network_state', sequence: violation === 'duplicate_sequence' ? 1 : 2, profile: violation === 'wrong_profile' ? 'VNEXT_PRIVATE_WEBSOCKET_PROFILE_V1' : ready.profile, data: violation === 'private_field' ? { ...aggregate, expected_peer: peer } : aggregate };
    socket.onmessage({ data: JSON.stringify(event) });
    expect(hint).not.toHaveBeenCalled(); expect(gap).toHaveBeenCalledTimes(1); expect(socket.close).toHaveBeenCalledTimes(1);
  });
  it('uses scoped tickets, accepts aggregate hints, closes on gaps, and never dispatches a mutation', async () => {
    const calls: any[] = []; const urls: string[] = [];
    const ticket = 'obw1.' + 'a'.repeat(43), clientSession = 'obw1.' + 'b'.repeat(43);
    const client = createObpClient(async () => ({ baseUrl: origin, token }), async (url, init) => {
      calls.push({ url, init });
      return String(url).endsWith('/tickets') ? response({ ticket, client_session: clientSession, expires_at: 30, session_expires_at: 900, subscriptions: ['network'], limitations: [] }) : response({ available: true, session, status });
    });
    const socket = { onmessage: null as any, onclose: null as any, onerror: null as any, close: vi.fn() };
    const hint = vi.fn(), gap = vi.fn();
    const stop = await client.subscribe(session, hint, gap, url => { urls.push(url); return socket as any; });
    const event = (sequence: number, event_type: string, data: unknown) => ({ data: JSON.stringify({ profile: 'OBP_PRIVATE_WEBSOCKET_PROFILE_V1', sequence, event_type, timestamp: 1, lifecycle: 'disabled', coverage: 'partial', limitations: [], data }) });
    socket.onmessage(event(1, 'subscription_ready', { subscriptions: ['network'], session_expires_at: 900 }));
    await client.status(); expect(calls[1].init.headers['X-OneBrain-VNext-Client-Session']).toBe(clientSession);
    const aggregate = Object.fromEntries(Object.entries(status).filter(([k]) => !['generation', 'lifecycle', 'coverage', 'limitations'].includes(k)));
    socket.onmessage(event(2, 'network_state', aggregate)); expect(hint).toHaveBeenCalledTimes(1);
    socket.onmessage(event(4, 'network_state', aggregate)); expect(gap).toHaveBeenCalledTimes(1); expect(socket.close).toHaveBeenCalledTimes(1);
    await client.status(); expect(calls[2].init.headers['X-OneBrain-VNext-Client-Session']).toBeUndefined();
    stop(); expect(urls[0]).toBe(`${origin.replace('http', 'ws')}/api/vnext/obp/ws?ticket=${ticket}`);
    expect(calls.every(c => !String(c.url).includes('/commands'))).toBe(true);
    expect(() => fields({ ...aggregate, expected_peer: peer }, Object.keys(aggregate))).toThrow();
  });
});
