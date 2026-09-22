import { useCallback, useEffect, useRef, useState } from 'react';
import { getPrivateApiConnection } from '../api/client';
import { createObpClient, ObpError, readRecovery } from '../api/obp';
import type { ObpClient, Page, Recovery, Result, Snapshot } from '../api/obp';
import { hexId, operations, sourceKinds, validate } from '../api/obpContract';
import type { Fields } from '../api/obpContract';
import './obpNetwork.css';

const defaultClient = createObpClient(getPrivateApiConnection);
export const recoveryKey = 'obp.web.pending.v1';
const labels: Record<string, string> = {
  configure: 'Save network settings', source_admit: 'Import host reference', source_set_enabled: 'Change source policy',
  refresh: 'Request discovery refresh', route_request: 'Connect expected peer', intent_retry: 'Schedule pending intent retry',
  network_kill: 'Kill networking', network_reenable: 'Re-enable generation',
};
function Details({ value }: { value: Fields }) {
  return <dl className="obp-details">{Object.entries(value).filter(([k]) => !['items', 'claims_global_completion', 'authorizes_reward'].includes(k)).map(([k, v]) => <div key={k}><dt>{k.replaceAll('_', ' ')}</dt><dd>{Array.isArray(v) ? (v.length ? v.join(', ') : 'None reported') : String(v)}</dd></div>)}</dl>;
}
function identifier() { return Array.from(crypto.getRandomValues(new Uint8Array(32)), b => b.toString(16).padStart(2, '0')).join(''); }

export function NetworkPage({ client = defaultClient }: { client?: ObpClient }) {
  const [snapshot, setSnapshot] = useState<Result<Snapshot>>();
  const snapshotRef = useRef<Result<Snapshot> | undefined>(undefined);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false); const locked = useRef(false);
  const [stream, setStream] = useState('Not connected');
  const stopStream = useRef<(() => void) | undefined>(undefined);
  const mounted = useRef(true); const readEpoch = useRef(0);
  const [sources, setSources] = useState<Page>(); const [reservations, setReservations] = useState<Page>();
  const [route, setRoute] = useState<Fields>(); const [intent, setIntent] = useState<Fields>();
  const [peerId, setPeerId] = useState(''); const [routeId, setRouteId] = useState(''); const [intentId, setIntentId] = useState('');
  const [inputRef, setInputRef] = useState(''); const [kind, setKind] = useState('manual_invitation');
  const [management, setManagement] = useState('');
  const [requested, setRequested] = useState(false); const [advertise, setAdvertise] = useState(false); const [previewed, setPreviewed] = useState(false);
  const [preview, setPreview] = useState<Recovery>(); const [confirmation, setConfirmation] = useState('');
  const confirmInput = useRef<HTMLInputElement>(null);
  const [pending, setPending] = useState<Recovery>(); const [outcome, setOutcome] = useState<Fields>(); const [storageBlocked, setStorageBlocked] = useState(false);
  const available = snapshot?.data.available ? snapshot.data : undefined;
  const canManage = /^obm1\.[A-Za-z0-9_-]{43}$/.test(management);
  const canMutate = !!available && !busy && !pending && !storageBlocked;
  useEffect(() => { if (preview) { confirmInput.current?.focus(); confirmInput.current?.scrollIntoView?.({ block: 'center' }); } }, [preview]);
  const clearDetails = useCallback(() => { setSources(undefined); setReservations(undefined); setRoute(undefined); setIntent(undefined); }, []);
  const refresh = useCallback(async () => {
    const epoch = ++readEpoch.current;
    clearDetails(); setPreview(undefined); setConfirmation('');
    try {
      const next = await client.status();
      if (!mounted.current || epoch !== readEpoch.current) return;
      const old = snapshotRef.current?.data;
      if (old?.available && (!next.data.available || JSON.stringify(old.session) !== JSON.stringify(next.data.session))) {
        stopStream.current?.(); stopStream.current = undefined; setStream('Session changed; reconnect private hints'); setManagement('');
      }
      snapshotRef.current = next; setSnapshot(next);
    } catch (e) {
      if (!mounted.current || epoch !== readEpoch.current) return;
      snapshotRef.current = undefined; setSnapshot(undefined); stopStream.current?.(); stopStream.current = undefined;
      setError(e instanceof ObpError ? e.message : 'Unable to read local OBP state');
    }
  }, [client, clearDetails]);
  const run = async (action: () => Promise<void>) => {
    if (locked.current) return; locked.current = true; setBusy(true); setError('');
    try { await action(); } catch (e) { setError(e instanceof ObpError ? e.message : 'Invalid input or unavailable local projection. Refresh observations.'); }
    finally { locked.current = false; if (mounted.current) setBusy(false); }
  };
  useEffect(() => {
    mounted.current = true;
    try { const stored = sessionStorage.getItem(recoveryKey); if (stored) setPending(readRecovery(stored)); }
    catch { setStorageBlocked(true); setError('Recovery storage is unavailable or invalid. Commands are blocked; preserve this tab for operator recovery.'); }
    void refresh();
    // This ref owns the latest socket, not a DOM node captured at mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    return () => { mounted.current = false; readEpoch.current++; stopStream.current?.(); stopStream.current = undefined; };
  }, [refresh]);
  async function connectHints() {
    if (!available) return;
    stopStream.current?.(); setStream('Connecting private hints');
    const epoch = readEpoch.current;
    let refreshing = false, anotherHint = false;
    const readHint = async () => {
      if (refreshing) { anotherHint = true; return; }
      refreshing = true;
      do { anotherHint = false; await refresh(); } while (anotherHint && mounted.current);
      refreshing = false;
    };
    const stop = await client.subscribe(available.session, () => { void readHint(); }, () => {
      stopStream.current = undefined; setStream('Stream gap — observations refreshed; reconnect explicitly'); void refresh();
    });
    if (!mounted.current || epoch !== readEpoch.current) { stop(); return; }
    stopStream.current = stop; setStream('Private hints requested; REST remains authoritative');
    await refresh();
  }
  async function prepare(operation: string, extra: Fields = {}) {
    if (!available || pending || storageBlocked) return;
    const record = { origin: await client.origin(), session: { ...available.session }, operation,
      payload: { idempotency_key: identifier(), expected_generation: available.status.generation, ...extra } };
    validate(operations.find(o => o.name === operation)!.request, record.payload);
    setConfirmation(''); setPreview(record); setOutcome(undefined);
  }
  async function dispatch() {
    if (!preview || confirmation !== preview.payload.idempotency_key || pending) return;
    if (operations.find(o => o.name === preview.operation)?.access === 'host_management' && !canManage) {
      throw new ObpError('Host-issued management capability required before confirmation');
    }
    sessionStorage.setItem(recoveryKey, JSON.stringify(preview)); setPending(preview); setPreview(undefined); setConfirmation('');
    const record = preview;
    try {
      const result = await client.command(record, management); setOutcome(result.data);
      if (result.data.state === 'completed') {
        await refresh();
        const current = snapshotRef.current?.data;
        if (current?.available && JSON.stringify(current.session) === JSON.stringify(record.session) && current.status.generation === record.payload.expected_generation) {
          if (record.operation === 'route_request') setRoute(result.data.result as Fields);
          if (record.operation === 'intent_retry') setIntent(result.data.result as Fields);
        }
      }
    } catch (e) {
      if (e instanceof ObpError && !e.uncertain) setOutcome({ state: 'not_admitted', reason: e.reason });
      throw e;
    } finally { setManagement(''); }
  }
  async function loadPage(which: 'source_list' | 'reservation_list', next = false) {
    if (!available) return;
    const page = which === 'source_list' ? sources : reservations;
    const epoch = readEpoch.current;
    const setter = which === 'source_list' ? setSources : setReservations;
    try {
      const r = await client.query<Page>(available.session, which, { limit: 32, ...(next && page?.continuation ? { continuation: page.continuation } : {}) });
      if (epoch !== readEpoch.current) return;
      if (next && page && r.data.snapshot_frontier !== page.snapshot_frontier) throw new ObpError('Snapshot changed; load first page explicitly');
      setter(r.data);
    } catch (e) { setter(undefined); throw e; }
  }
  async function inspect(which: 'route_status' | 'intent_status') {
    if (!available) return; const epoch = readEpoch.current;
    if (which === 'route_status') setRoute(undefined); else setIntent(undefined);
    const r = await client.query(available.session, which, which === 'route_status' ? { route_id: routeId } : { intent_id: intentId });
    if (epoch !== readEpoch.current) return;
    if (which === 'route_status') setRoute(r.data); else setIntent(r.data);
  }
  const button = (operation: string, extra: Fields = {}, disabled = false, title?: string) => <button className="btn" disabled={!canMutate || disabled || (operations.find(o => o.name === operation)?.access === 'host_management' && !canManage)} onClick={() => void run(() => prepare(operation, extra))}>{title || labels[operation]}</button>;
  const resolved = ['completed', 'failed_no_effect', 'not_admitted'].includes(String(outcome?.state));
  return <div className="page obp-network" aria-busy={busy}>
    <header className="page-header"><h1>Network</h1><p>Local node observations · outbound reachability</p></header>
    <p>DNS/IP locates a source. Cryptographic identity authenticates the exact peer. A relay provides availability, not trust or delivery. These bounded observations never prove global completeness or authorize rewards. KU remains usable offline.</p>
    <div className="obp-actions"><button className="btn" disabled={busy} onClick={() => void run(refresh)}>Refresh observations</button><button className="btn" disabled={busy || !available} onClick={() => void run(connectHints)}>Connect private hints</button></div>
    <p role="status">{stream}</p><div role="alert">{error}</div>
    <section className="glass-card" aria-labelledby="obp-status"><h2 id="obp-status">Shared node state</h2>
      {!snapshot ? <p>Local state unavailable or loading. No connection or completion is inferred.</p> : !available ? <p>OBP service unavailable. Compiled: {String(!snapshot.data.available && snapshot.data.compiled)}. The trusted host must install the shared service and bindings; this page cannot activate it.</p> : <><Details value={available.status} /><p>Session and durable network generation are checked for every operation.</p></>}
      {snapshot && <p>Scope: {snapshot.meta.lifecycle} · {snapshot.meta.coverage}. Limitations: {snapshot.meta.limitations.join(', ') || 'None reported'}.</p>}
    </section>
    {(pending || preview) && <section className="glass-card" aria-labelledby="obp-command"><h2 id="obp-command">{pending ? 'Command recovery' : 'Confirm exact command'}</h2>
      <p>{labels[(pending || preview)!.operation]} · original payload and generation</p><pre>{JSON.stringify((pending || preview)!.payload, null, 2)}</pre>
      {preview && !pending && <><p>This action can change local policy or schedule bounded traffic. Confirm only the previewed action.</p><label htmlFor="obp-confirm">Type the exact idempotency key</label><input ref={confirmInput} id="obp-confirm" className="input" maxLength={64} autoComplete="off" value={confirmation} onChange={e => setConfirmation(e.target.value)} /><div className="obp-actions"><button className="btn" disabled={busy || confirmation !== preview.payload.idempotency_key} onClick={() => void run(dispatch)}>Confirm command</button><button className="btn" disabled={busy} onClick={() => setPreview(undefined)}>Discard preview</button></div></>}
      {pending && <><p>Retained in this tab for refresh/restart recovery. No automatic replay. An unknown or missing outcome does not establish that an effect never happened.</p><button className="btn" disabled={busy} onClick={() => void run(async () => { const r = await client.reconcile(pending); setOutcome(r.data); await refresh(); })}>Reconcile original key</button>
        {outcome && <><Details value={Object.fromEntries(Object.entries(outcome).filter(([k]) => !['result', 'failure'].includes(k)))} /><p>{outcome.state === 'completed' ? 'Recorded command completed. Metadata alone does not supply its result or prove delivery. Inspect current route/intent separately.' : outcome.state === 'failed_no_effect' || outcome.state === 'not_admitted' ? 'This attempt reports no effect. Review the failure before preparing a new action.' : 'Outcome unresolved. Preserve this key; do not resubmit.'}</p>{!!outcome.failure && <Details value={outcome.failure as Fields} />}</>}
        {resolved && <button className="btn" disabled={busy} onClick={() => void run(async () => { sessionStorage.removeItem(recoveryKey); setPending(undefined); setOutcome(undefined); })}>Acknowledge recorded outcome</button>}</>}
    </section>}
    <div className="obp-grid">
      <section className="glass-card" aria-labelledby="obp-sources"><h2 id="obp-sources">Bootstrap and source health</h2><p>Import uses an existing host-issued input reference. File/QR/pasted invitation intake is unavailable here; the trusted host must validate and register it first.</p>
        <label htmlFor="obp-kind">Source kind</label><select id="obp-kind" className="input" value={kind} onChange={e => setKind(e.target.value)}>{sourceKinds.map(k => <option key={k} value={k}>{k.replaceAll('_', ' ')}</option>)}</select>
        <label htmlFor="obp-input">Host input reference (64 hex)</label><input id="obp-input" className="input" autoComplete="off" value={inputRef} onChange={e => setInputRef(e.target.value)} />
        <div className="obp-actions">{button('source_admit', { input_ref: inputRef, kind }, !hexId(inputRef))}{button('refresh')}</div>
        <div className="obp-actions"><button className="btn" disabled={!available || busy} onClick={() => void run(() => loadPage('source_list'))}>Load sources</button><button className="btn" disabled={!sources?.continuation || busy} onClick={() => void run(() => loadPage('source_list', true))}>Next sources page</button></div>
        {sources && <><Details value={sources} />{sources.items.length === 0 && <p>No sources in this local snapshot.</p>}{sources.items.map(s => <article key={String(s.source_id)}><Details value={s} /><div className="obp-actions">{button('source_set_enabled', { source_id: s.source_id, enabled: true }, false, 'Enable source')}{button('source_set_enabled', { source_id: s.source_id, enabled: false }, false, 'Disable source')}</div></article>)}</>}
        <p>Disabling preserves replay floors; it does not delete a source.</p>
      </section>
      <section className="glass-card" aria-labelledby="obp-reservations"><h2 id="obp-reservations">Relay reservations</h2><p>An active reservation is a usable outer carrier, not an authenticated target peer. Fewer than two usable reservations is partial coverage.</p><div className="obp-actions"><button className="btn" disabled={!available || busy} onClick={() => void run(() => loadPage('reservation_list'))}>Load reservations</button><button className="btn" disabled={!reservations?.continuation || busy} onClick={() => void run(() => loadPage('reservation_list', true))}>Next reservations page</button></div>
        {reservations && <><Details value={reservations} />{reservations.items.length === 0 && <p>No reservations in this local snapshot.</p>}{reservations.items.map(s => <article key={String(s.relay_node_id)}><Details value={s} /></article>)}</>}
      </section>
      <section className="glass-card" aria-labelledby="obp-route"><h2 id="obp-route">Expected peer and candidate path</h2><p>Inspect a known route or request the exact full NodeID. The API provides no peer directory or raw candidate addresses.</p><label htmlFor="obp-peer">Expected peer NodeID (64 hex)</label><input id="obp-peer" className="input" autoComplete="off" value={peerId} onChange={e => setPeerId(e.target.value)} /><div className="obp-actions">{button('route_request', { expected_peer: peerId }, !hexId(peerId))}</div>
        <label htmlFor="obp-route-id">Route ID (64 hex)</label><input id="obp-route-id" className="input" autoComplete="off" value={routeId} onChange={e => setRouteId(e.target.value)} /><button className="btn" disabled={!available || busy || !hexId(routeId)} onClick={() => void run(() => inspect('route_status'))}>Inspect route</button>
        {route && <Details value={route} />}<p>Path-limited means no authenticated route within this attempt's budget. It does not mean the peer is globally offline. A connected route sends no application payload.</p>
      </section>
      <section className="glass-card" aria-labelledby="obp-intent"><h2 id="obp-intent">Durable intent and failover</h2><p>Use an intent ID from its originating workflow; no outbox-list or cancel endpoint exists. The node owns alternate relay selection, fresh peer authentication and exact checkpoint resume.</p><label htmlFor="obp-intent-id">Intent ID (64 hex)</label><input id="obp-intent-id" className="input" autoComplete="off" value={intentId} onChange={e => { setIntentId(e.target.value); setIntent(undefined); }} /><button className="btn" disabled={!available || busy || !hexId(intentId)} onClick={() => void run(() => inspect('intent_status'))}>Inspect intent</button>
        {intent && <><Details value={intent} /><div className="obp-actions">{button('intent_retry', { intent_id: intent.intent_id }, intent.state !== 'pending')}</div></>}<p>Retry schedules existing pending bytes; counters, consent and checkpoint are preserved. Terminal intents cannot be resurrected. A socket write or relay receipt is not acknowledgement.</p>
      </section>
    </div>
    <section className="glass-card" aria-labelledby="obp-management"><h2 id="obp-management">Host management</h2><p>Control and scoped management grants must be installed by the trusted host. The API Bearer alone grants no management authority. A grant expires within five minutes; this page cannot mint it.</p>
      <label htmlFor="obp-management-token">Host-issued management capability</label><input id="obp-management-token" className="input" type="password" autoComplete="off" value={management} onChange={e => setManagement(e.target.value)} /><p>{canManage ? 'Capability supplied; the host still checks scope, expiry and revocation.' : 'Management unavailable until a host-issued capability is supplied.'}</p>
      <fieldset><legend>Independent opt-ins</legend><label><input type="checkbox" checked={requested} onChange={e => setRequested(e.target.checked)} /> Request outbound reachability</label><label><input type="checkbox" checked={advertise} onChange={e => { setAdvertise(e.target.checked); setPreviewed(false); }} /> Advertise reachability</label>
        {advertise && <label><input type="checkbox" checked={previewed} onChange={e => setPreviewed(e.target.checked)} /> I reviewed the exact public fields and expiry supplied by the trusted host before it granted this authority.</label>}
        <p>Advertising publishes a short-lived signed reachability object, not KU content or permanent disclosure. Requested configuration may require a host restart.</p>{button('configure', { outbound_first_requested: requested, advertise_reachability: advertise }, advertise && !previewed)}
      </fieldset><div className="obp-actions">{button('network_kill')}{button('network_reenable')}</div><p>Kill fences new network work and preserves durable intents. Re-enable advances the generation; it does not enable other lanes.</p>
    </section>
  </div>;
}
