import { useEffect, useState } from 'react';
import { AlertTriangle, Archive, Cpu, Play, Radar, RefreshCw, ShieldAlert } from 'lucide-react';
import { api, VNextApiError } from '../api/client';
import { p10Api, type CompiledNeedIrView } from '../api/p10Client';
import type {
  MatchPage,
  NeedPage,
  NeedView,
  PreparedNeed,
  VNextBudget,
  VNextMeta,
} from '../api/types';
import {
  COVERAGE_LOCAL_STORE_ONLY,
  PROVENANCE_PRIVATE_LOCAL_ONLY,
  PROVENANCE_PUBLIC_UNASSESSED,
  USABLE_OFFLINE_BADGE,
  formatCoverageBadge,
  formatNeedCompilerBadge,
} from '../utils/epistemicBadges';

const DEFAULT_BUDGET: VNextBudget = {
  max_scan_records: 4096,
  max_affordances: 1024,
  max_pairs: 65536,
  max_proposals: 4096,
};

function operationKey(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

function explainError(reason: unknown): string {
  if (reason instanceof VNextApiError) {
    return `${reason.code}: ${reason.message} (${reason.meta.lifecycle}, ${reason.meta.coverage})`;
  }
  return reason instanceof Error ? reason.message : String(reason);
}

export function OneHopDiscovery() {
  const [query, setQuery] = useState('FIND (k:KU) SCOPE LOCAL LIMIT 20');
  const [prepared, setPrepared] = useState<PreparedNeed | null>(null);
  const [compiledNeed, setCompiledNeed] = useState<CompiledNeedIrView | null>(null);
  const [prepareKey, setPrepareKey] = useState('');
  const [needs, setNeeds] = useState<NeedPage | null>(null);
  const [selected, setSelected] = useState<NeedView | null>(null);
  const [matches, setMatches] = useState<MatchPage | null>(null);
  const [scanContinuation, setScanContinuation] = useState<string | undefined>();
  const [meta, setMeta] = useState<VNextMeta | null>(null);
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');

  const refreshNeeds = async () => {
    setBusy('list');
    setError('');
    try {
      const result = await api.listNeeds();
      setNeeds(result.data);
      setMeta(result.meta);
    } catch (reason) {
      setError(explainError(reason));
    } finally {
      setBusy('');
    }
  };

  useEffect(() => {
    void refreshNeeds();
  }, []);

  const compileNeed = async () => {
    if (!query.trim()) return;
    setBusy('compile');
    setError('');
    try {
      const result = await p10Api.compileNeedIr({ local_query: query.trim() });
      setCompiledNeed(result.data);
    } catch (reason) {
      setCompiledNeed(null);
      setError(explainError(reason));
    } finally {
      setBusy('');
    }
  };

  const prepare = async () => {
    if (!query.trim()) return;
    const key = operationKey('web-need');
    setBusy('prepare');
    setError('');
    try {
      const result = await api.prepareNeed({
        local_query: query.trim(),
        scope: { kind: 'one_hop', max_hops: 1, node_ids: [] },
        budget: DEFAULT_BUDGET,
        idempotency_key: key,
      });
      setPrepared(result.data);
      setPrepareKey(key);
      setMeta(result.meta);
    } catch (reason) {
      setError(explainError(reason));
    } finally {
      setBusy('');
    }
  };

  const activate = async () => {
    if (!prepared) return;
    setBusy('activate');
    setError('');
    try {
      const result = await api.activateNeed(prepared.intent_cid, prepareKey);
      setSelected(result.data);
      setPrepared(null);
      setMeta(result.meta);
      await refreshNeeds();
    } catch (reason) {
      setError(explainError(reason));
      setBusy('');
    }
  };

  const scan = async () => {
    if (!selected) return;
    setBusy('scan');
    setError('');
    try {
      const result = await api.scanNeed(
        selected.standing_need_id,
        DEFAULT_BUDGET,
        operationKey('web-scan'),
        scanContinuation,
      );
      setSelected(result.data);
      setScanContinuation(result.meta.continuation || undefined);
      setMeta(result.meta);
      await loadMatches(result.data);
    } catch (reason) {
      setError(explainError(reason));
      setBusy('');
    }
  };

  const loadMatches = async (need = selected, continuation?: string) => {
    if (!need) return;
    setBusy('matches');
    setError('');
    try {
      const result = await api.listNeedMatches(need.standing_need_id, continuation);
      setSelected(need);
      setMatches(result.data);
      setMeta(result.meta);
    } catch (reason) {
      setError(explainError(reason));
    } finally {
      setBusy('');
    }
  };

  const retire = async () => {
    if (!selected) return;
    setBusy('retire');
    setError('');
    try {
      const result = await api.retireNeed(selected.standing_need_id);
      setSelected(result.data);
      setMatches(null);
      setMeta(result.meta);
      await refreshNeeds();
    } catch (reason) {
      setError(explainError(reason));
      setBusy('');
    }
  };

  const observedPeerIds = new Set(
    matches?.items.flatMap(m => m.responder_scope.node_ids) ?? [],
  );
  const coverageBadge = formatCoverageBadge({
    coverage: meta?.coverage ?? 'local_only',
    reachablePeers: observedPeerIds.size,
  });

  return (
    <div style={{ display: 'grid', gap: 18 }}>
      <div className="glass-card" style={{ borderColor: 'rgba(99,102,241,0.45)' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 8 }}>
          <h2 style={{ fontSize: '1rem', display: 'flex', gap: 8, alignItems: 'center' }}>
            <Radar size={18} /> One-hop discovery
          </h2>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
            <span className="badge badge-green" style={{ textTransform: 'none' }}>
              {USABLE_OFFLINE_BADGE}
            </span>
            <span className="badge badge-cyan" style={{ textTransform: 'none' }}>
              {PROVENANCE_PRIVATE_LOCAL_ONLY}
            </span>
            <span className="badge badge-violet" style={{ textTransform: 'none' }}>
              {coverageBadge}
            </span>
          </div>
        </div>
        <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.82rem', margin: '6px 0 14px' }}>
          Creates a private, bounded StandingNeed over authenticated direct paths. Results are partial
          proposals and are never executable or automatically adopted.
        </p>
        <textarea
          className="input mono"
          rows={4}
          value={query}
          onChange={event => setQuery(event.target.value)}
          aria-label="One-hop KQL query"
          style={{ width: '100%', resize: 'vertical' }}
        />
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 10 }}>
          <button className="btn btn-primary" onClick={() => void prepare()} disabled={!!busy}>
            {busy === 'prepare' ? <span className="spinner" /> : <><ShieldAlert size={15} /> Prepare private Need</>}
          </button>
          <button className="btn" onClick={() => void compileNeed()} disabled={!!busy}>
            {busy === 'compile' ? <span className="spinner" /> : <><Cpu size={15} /> Compile NeedIR (KQL-CMP-001)</>}
          </button>
        </div>
      </div>

      {compiledNeed && (
        <div className="glass-card" aria-label="Compiled NeedIR preview">
          <h3>
            Deterministic FIND → NeedIR Preview{' '}
            <span className="badge badge-cyan" style={{ textTransform: 'none', marginLeft: 6 }}>
              {formatNeedCompilerBadge({
                compiled: compiledNeed.compiled,
                compilerProfile: compiledNeed.compiler_profile,
                limitations: compiledNeed.limitations,
              })}
            </span>
          </h3>
          <Detail label="Compiler profile / Registry root" value={`${compiledNeed.compiler_profile} / ${compiledNeed.registry_release_root}`} />
          <Detail label="ReceptorDefinition CID" value={compiledNeed.receptor_definition_cid} />
          <Detail label="QueryDefinition CID" value={compiledNeed.query_definition_cid} />
          <Detail label="Allow-listed Selector CID" value={`${compiledNeed.selector_cid} (${compiledNeed.selector_policy})`} />
          <Detail label="Raw query zeroized" value={`${compiledNeed.raw_query_zeroized} · ${PROVENANCE_PRIVATE_LOCAL_ONLY} · ${COVERAGE_LOCAL_STORE_ONLY}`} />
        </div>
      )}

      {error && (
        <div className="glass-card" style={{ color: 'var(--ob-error)', borderColor: 'rgba(239,68,68,.4)' }}>
          <AlertTriangle size={16} style={{ verticalAlign: 'middle', marginRight: 8 }} />{error}
        </div>
      )}

      {prepared && (
        <div className="glass-card">
          <h3>Prepared locally — no peer request sent yet</h3>
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginTop: 6 }}>
            <span className="badge badge-cyan" style={{ textTransform: 'none' }}>
              {PROVENANCE_PRIVATE_LOCAL_ONLY}
            </span>
            <span className="badge badge-violet" style={{ textTransform: 'none' }}>
              {formatNeedCompilerBadge({ limitations: prepared.limitations })}
            </span>
          </div>
          <Detail label="Intent CID" value={prepared.intent_cid} />
          <Detail label="Private query definition" value={prepared.query_definition_cid} />
          <Detail label="Selector" value={prepared.selector_cid} />
          <Detail label="Responder scope" value={`${prepared.scope.kind}; max_hops=${prepared.scope.max_hops}`} />
          <Detail label="Expiry" value={new Date(prepared.expires_at * 1000).toLocaleString()} />
          <Limitations values={prepared.limitations} />
          <button className="btn btn-primary" disabled={!!busy} onClick={() => void activate()} style={{ marginTop: 12 }}>
            <Play size={15} /> Activate exact prepared Need
          </button>
        </div>
      )}

      <div className="glass-card">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h3>Private StandingNeeds on this node</h3>
          <button className="btn btn-ghost" onClick={() => void refreshNeeds()} disabled={!!busy}>
            <RefreshCw size={14} /> Refresh
          </button>
        </div>
        {needs?.items.length === 0 && (
          <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.82rem' }}>
            No active StandingNeed is stored locally. This is not a network-wide absence claim.
          </p>
        )}
        <div style={{ display: 'grid', gap: 8, marginTop: 10 }}>
          {needs?.items.map(need => (
            <button
              key={need.standing_need_id}
              className="btn btn-ghost"
              onClick={() => void loadMatches(need)}
              style={{ justifyContent: 'space-between', textAlign: 'left' }}
            >
              <span className="mono">{need.standing_need_id.slice(0, 18)}…</span>
              <span>{need.state} · revision {need.revision}</span>
            </button>
          ))}
        </div>
        {needs && <Continuation value={needs.continuation} />}
      </div>

      {selected && (
        <div className="glass-card">
          <h3>Selected one-hop Need</h3>
          <Detail label="StandingNeed" value={selected.standing_need_id} />
          <Detail label="Selector" value={selected.selector_cid} />
          <Detail label="Coverage" value={selected.coverage} />
          <Detail label="State / revision" value={`${selected.state} / ${selected.revision}`} />
          <Limitations values={selected.limitations} />
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 12 }}>
            <button className="btn btn-primary" onClick={() => void scan()} disabled={!!busy || selected.state === 'retired'}>
              <Radar size={15} /> Bounded scan
            </button>
            <button className="btn btn-ghost" onClick={() => void loadMatches()} disabled={!!busy}>
              <RefreshCw size={15} /> Refresh proposals
            </button>
            <button className="btn btn-ghost" onClick={() => void retire()} disabled={!!busy || selected.state === 'retired'}>
              <Archive size={15} /> Retire
            </button>
          </div>
          <Continuation label="Scan continuation" value={scanContinuation || null} />
        </div>
      )}

      {matches && (
        <div className="glass-card">
          <h3>Bounded match projection</h3>
          {matches.items.length === 0 && (
            <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.82rem' }}>
              No proposal in the assessed local/one-hop frontier. Coverage outside this assessed
              frontier is unknown.
            </p>
          )}
          <div style={{ display: 'grid', gap: 12, marginTop: 10 }}>
            {matches.items.map(item => (
              <article key={item.proposal_cid} style={{
                padding: 14,
                border: '1px solid rgba(245,158,11,.45)',
                borderRadius: 10,
                background: 'rgba(245,158,11,.06)',
              }}>
                <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'center' }}>
                  <span className="badge" style={{ color: 'var(--ob-warning)' }}>
                    quarantined proposal
                  </span>
                  <span className="badge badge-amber" style={{ textTransform: 'none' }}>
                    {PROVENANCE_PUBLIC_UNASSESSED}
                  </span>
                </div>
                <Detail label="Proposal / candidate" value={`${item.proposal_cid} / ${item.candidate_cid}`} />
                <Detail
                  label="Responder scope"
                  value={`${item.responder_scope.kind}; max_hops=${item.responder_scope.max_hops}; peers=${item.responder_scope.node_ids.join(', ') || 'authenticated path'}`}
                />
                <Detail label="Selector" value={item.selector_cid} />
                <Detail label="Assessed frontier" value={item.assessed_frontier} />
                <Detail
                  label="Constraints"
                  value={`${item.constraints.all_required_satisfied ? 'observed satisfied' : 'unresolved'}; executable=${item.executable}`}
                />
                <Limitations values={item.limitations} />
              </article>
            ))}
          </div>
          <Continuation value={matches.continuation} />
          {matches.continuation && (
            <button
              className="btn btn-ghost"
              onClick={() => void loadMatches(selected, matches.continuation || undefined)}
              disabled={!!busy}
            >
              Load next bounded page
            </button>
          )}
          <Limitations values={matches.limitations} />
        </div>
      )}

      {meta && (
        <div style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)' }}>
          Runtime: {meta.lifecycle} · Coverage: {meta.coverage}
          <Limitations values={meta.limitations} />
        </div>
      )}
    </div>
  );
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ marginTop: 8 }}>
      <span style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.72rem' }}>{label}</span>
      <div className="mono" style={{ fontSize: '0.72rem', overflowWrap: 'anywhere' }}>{value}</div>
    </div>
  );
}

function Continuation({ label = 'Page continuation', value }: { label?: string; value: string | null }) {
  return (
    <Detail
      label={label}
      value={value || 'none — current bounded page is complete; coverage remains partial'}
    />
  );
}

function Limitations({ values }: { values: string[] }) {
  if (values.length === 0) return null;
  return (
    <div style={{ marginTop: 8, color: 'var(--ob-text-tertiary)', fontSize: '0.72rem' }}>
      Limitations: {values.join(' · ')}
    </div>
  );
}
