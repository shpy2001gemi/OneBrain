import { useState } from 'react';
import { AlertTriangle, Eye, LockKeyhole, RefreshCw, Send, ShieldCheck } from 'lucide-react';
import { api, VNextApiError } from '../api/client';
import { deriveVNextConfirmationReceipt } from '../api/vnextReceipt';
import type {
  MetabolicEvidenceView,
  PreparedPublicUse,
  PublicationView,
  PublicUsePrepareRequest,
  VNextMeta,
  VNextUseMode,
} from '../api/types';

const USE_MODES: VNextUseMode[] = [
  'application',
  'transformation',
  'epistemic',
  'transfer',
  'discovery',
  'receptor_discovered',
  'candidate_evaluated',
  'constraint_clarified',
  'gap_partially_filled',
  'assembly_used',
  'analogical_transfer',
  'compared_or_opposed',
  'capability_result_used',
];

function explainError(reason: unknown): string {
  if (reason instanceof VNextApiError) {
    return `${reason.code}: ${reason.message} (${reason.meta.lifecycle}, ${reason.meta.coverage})`;
  }
  return reason instanceof Error ? reason.message : String(reason);
}

export function VNextPomv() {
  const [viewTarget, setViewTarget] = useState('');
  const [view, setView] = useState<MetabolicEvidenceView | null>(null);
  const [viewMeta, setViewMeta] = useState<VNextMeta | null>(null);
  const [targetCid, setTargetCid] = useState('');
  const [recipient, setRecipient] = useState('');
  const [selector, setSelector] = useState('');
  const [namespace, setNamespace] = useState('onebrain.public-use');
  const [useMode, setUseMode] = useState<VNextUseMode>('application');
  const [permanentAck, setPermanentAck] = useState(false);
  const [prepared, setPrepared] = useState<PreparedPublicUse | null>(null);
  const [typedIntent, setTypedIntent] = useState('');
  const [publication, setPublication] = useState<PublicationView | null>(null);
  const [publicationMeta, setPublicationMeta] = useState<VNextMeta | null>(null);
  const [publicationLookup, setPublicationLookup] = useState('');
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');

  const loadView = async () => {
    if (!viewTarget.trim()) return;
    setBusy('view');
    setError('');
    try {
      const result = await api.getMetabolicView(viewTarget.trim());
      setView(result.data);
      setViewMeta(result.meta);
    } catch (reason) {
      setView(null);
      setError(explainError(reason));
    } finally {
      setBusy('');
    }
  };

  const prepare = async () => {
    if (!permanentAck) return;
    setBusy('prepare');
    setError('');
    setPrepared(null);
    setTypedIntent('');
    setPublication(null);
    try {
      const input: PublicUsePrepareRequest = {
        target_cid: targetCid.trim(),
        recipient_node_id: recipient.trim(),
        selector_cid: selector.trim(),
        namespace: namespace.trim(),
        disclosure: {
          classification: 'public',
          permanent: true,
          use_mode: useMode,
        },
        idempotency_key: `web-public-use-${crypto.randomUUID()}`,
        expires_at: Math.floor(Date.now() / 1000) + 15 * 60,
      };
      const result = await api.preparePublicUse(input);
      setPrepared(result.data);
      setPublicationMeta(result.meta);
    } catch (reason) {
      setError(explainError(reason));
    } finally {
      setBusy('');
    }
  };

  const confirm = async () => {
    if (!prepared || typedIntent !== prepared.intent_cid) return;
    setBusy('confirm');
    setError('');
    try {
      const receipt = deriveVNextConfirmationReceipt(prepared.intent_cid);
      const result = await api.confirmPublicUse(prepared.intent_cid, receipt);
      setPublication(result.data);
      setPublicationLookup(result.data.publication_cid);
      setPublicationMeta(result.meta);
    } catch (reason) {
      setError(explainError(reason));
    } finally {
      setBusy('');
    }
  };

  const refreshPublication = async () => {
    if (!publicationLookup.trim()) return;
    setBusy('publication');
    setError('');
    try {
      const result = await api.getPublication(publicationLookup.trim());
      setPublication(result.data);
      setPublicationMeta(result.meta);
    } catch (reason) {
      setError(explainError(reason));
    } finally {
      setBusy('');
    }
  };

  return (
    <div style={{ display: 'grid', gap: 18 }}>
      {error && (
        <div className="glass-card" style={{ color: 'var(--ob-error)', borderColor: 'rgba(239,68,68,.4)' }}>
          <AlertTriangle size={16} style={{ verticalAlign: 'middle', marginRight: 8 }} />{error}
        </div>
      )}

      <section className="glass-card">
        <h2 style={{ fontSize: '1rem', display: 'flex', gap: 8, alignItems: 'center' }}>
          <Eye size={18} /> vNext Metabolic Evidence View
        </h2>
        <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.82rem' }}>
          Read-only, policy/frontier-relative evidence. Loading this view cannot create UseEvidence.
        </p>
        <div style={{ display: 'flex', gap: 8 }}>
          <input
            className="input mono"
            value={viewTarget}
            onChange={event => setViewTarget(event.target.value)}
            placeholder="64-character target CID"
            aria-label="Evidence view target CID"
            style={{ flex: 1 }}
          />
          <button className="btn btn-primary" onClick={loadView} disabled={!!busy}>
            {busy === 'view' ? <span className="spinner" /> : <><Eye size={15} /> Load view</>}
          </button>
        </div>

        {view && (
          <div style={{ marginTop: 16 }}>
            <div style={{
              padding: 12,
              borderRadius: 8,
              background: view.conflicts.length ? 'rgba(239,68,68,.08)' : 'rgba(99,102,241,.08)',
              border: `1px solid ${view.conflicts.length ? 'rgba(239,68,68,.4)' : 'rgba(99,102,241,.35)'}`,
            }}>
              <strong>
                {view.conflicts.length ? 'UNRESOLVED CONFLICT — not Authorized' : 'Partial evidence projection — not an authorization'}
              </strong>
            </div>
            <Detail label="Target / policy" value={`${view.target_cid} / ${view.policy_cid}`} />
            <Detail label="Assessed frontier" value={view.assessed_frontier} />
            <Detail label="Evidence root / revision" value={`${view.use_event_root} / ${view.revision}`} />
            <Detail label="Coverage" value={view.coverage} />
            <Detail
              label="Semantic flags"
              value={`truth=${view.establishes_truth}; benefit=${view.establishes_benefit}; reward=${view.authorizes_reward}; global_completion=${view.claims_global_completion}`}
            />
            {view.conflicts.length > 0 && (
              <Detail label="Unresolved conflicts" value={view.conflicts.join(' · ')} />
            )}
            <Limitations values={[...view.limitations, ...(viewMeta?.limitations || [])]} />
          </div>
        )}
      </section>

      <section className="glass-card" style={{ borderColor: 'rgba(245,158,11,.45)' }}>
        <h2 style={{ fontSize: '1rem', display: 'flex', gap: 8, alignItems: 'center' }}>
          <LockKeyhole size={18} /> Public Use wizard
        </h2>
        <p style={{ color: 'var(--ob-warning)', fontSize: '0.82rem' }}>
          Preparation is local and non-publishing. Confirmation permanently publishes the exact
          reviewed payload to the named recipient/outbox.
        </p>
        <div style={{ display: 'grid', gap: 9 }}>
          <Input label="Target CID" value={targetCid} setValue={setTargetCid} />
          <Input label="Recipient NodeID" value={recipient} setValue={setRecipient} />
          <Input label="Selector CID" value={selector} setValue={setSelector} />
          <Input label="Namespace" value={namespace} setValue={setNamespace} mono={false} />
          <label style={{ fontSize: '0.78rem', color: 'var(--ob-text-tertiary)' }}>
            Use mode
            <select
              className="input"
              value={useMode}
              onChange={event => setUseMode(event.target.value as VNextUseMode)}
              style={{ width: '100%', marginTop: 4 }}
            >
              {USE_MODES.map(mode => <option key={mode} value={mode}>{mode}</option>)}
            </select>
          </label>
          <label style={{ display: 'flex', gap: 9, alignItems: 'flex-start', fontSize: '0.82rem' }}>
            <input
              type="checkbox"
              checked={permanentAck}
              onChange={event => setPermanentAck(event.target.checked)}
            />
            I understand this disclosure is Public and permanent, and that delivery may remain pending or deferred.
          </label>
          <button className="btn btn-primary" onClick={prepare} disabled={!!busy || !permanentAck}>
            {busy === 'prepare' ? <span className="spinner" /> : <><ShieldCheck size={15} /> Prepare exact payload</>}
          </button>
        </div>
      </section>

      {prepared && (
        <section className="glass-card" style={{ borderColor: 'rgba(245,158,11,.55)' }}>
          <h3>Review exact prepared intent</h3>
          <Detail label="Intent CID" value={prepared.intent_cid} />
          <Detail label="Exact target" value={prepared.exact_target} />
          <Detail label="Exact recipient" value={prepared.exact_recipient} />
          <Detail label="Selector / namespace" value={`${prepared.selector_cid} / ${prepared.namespace}`} />
          <Detail
            label="Disclosure"
            value={`${prepared.disclosure.classification}; permanent=${prepared.disclosure.permanent}; mode=${prepared.disclosure.use_mode}`}
          />
          <Detail label="Idempotency key" value={prepared.idempotency_key} />
          <Detail label="Expires" value={new Date(prepared.expires_at * 1000).toLocaleString()} />
          <div style={{ marginTop: 10 }}>
            <div style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.72rem' }}>
              Exact canonical payload bytes (hex)
            </div>
            <pre style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', fontSize: '0.68rem', maxHeight: 260, overflow: 'auto' }}>
              {prepared.canonical_payload_preview}
            </pre>
          </div>
          <label style={{ display: 'block', marginTop: 12, fontSize: '0.78rem', color: 'var(--ob-text-tertiary)' }}>
            Type the exact intent CID to confirm
            <input
              className="input mono"
              value={typedIntent}
              onChange={event => setTypedIntent(event.target.value)}
              autoComplete="off"
              aria-label="Exact prepared intent confirmation"
              style={{ width: '100%', marginTop: 4 }}
            />
          </label>
          <button
            className="btn btn-primary"
            onClick={confirm}
            disabled={!!busy || typedIntent !== prepared.intent_cid}
            style={{ marginTop: 10 }}
          >
            {busy === 'confirm' ? <span className="spinner" /> : <><Send size={15} /> Confirm exact Public Use</>}
          </button>
          {typedIntent && typedIntent !== prepared.intent_cid && (
            <p style={{ color: 'var(--ob-error)', fontSize: '0.76rem' }}>
              Confirmation is disabled until the value exactly matches the prepared intent.
            </p>
          )}
        </section>
      )}

      <section className="glass-card">
        <h2 style={{ fontSize: '1rem' }}>Publication outbox status</h2>
        <div style={{ display: 'flex', gap: 8 }}>
          <input
            className="input mono"
            value={publicationLookup}
            onChange={event => setPublicationLookup(event.target.value)}
            placeholder="Publication CID"
            style={{ flex: 1 }}
          />
          <button className="btn btn-ghost" onClick={refreshPublication} disabled={!!busy}>
            <RefreshCw size={15} /> Refresh
          </button>
        </div>
        {!publication && (
          <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.8rem' }}>
            No publication selected. Retrieval is read-only and cannot create UseEvidence.
          </p>
        )}
        {publication && (
          <div style={{ marginTop: 12 }}>
            <span className="badge" style={{
              color: publication.state === 'deferred' ? 'var(--ob-warning)' : 'var(--ob-accent)',
            }}>
              outbox / {publication.state}
            </span>
            <Detail label="Publication / intent" value={`${publication.publication_cid} / ${publication.intent_cid}`} />
            <Detail label="Attempts / revision" value={`${publication.attempts} / ${publication.revision}`} />
            <Limitations values={[...publication.limitations, ...(publicationMeta?.limitations || [])]} />
          </div>
        )}
      </section>
    </div>
  );
}

function Input({
  label,
  value,
  setValue,
  mono = true,
}: {
  label: string;
  value: string;
  setValue: (value: string) => void;
  mono?: boolean;
}) {
  return (
    <label style={{ fontSize: '0.78rem', color: 'var(--ob-text-tertiary)' }}>
      {label}
      <input
        className={`input${mono ? ' mono' : ''}`}
        value={value}
        onChange={event => setValue(event.target.value)}
        style={{ width: '100%', marginTop: 4 }}
      />
    </label>
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

function Limitations({ values }: { values: string[] }) {
  const unique = [...new Set(values)];
  if (!unique.length) return null;
  return (
    <div style={{ marginTop: 8, color: 'var(--ob-text-tertiary)', fontSize: '0.72rem' }}>
      Limitations: {unique.join(' · ')}
    </div>
  );
}
