import { useState, useEffect, useCallback } from 'react';
import {
  Shield,
  KeyRound,
  QrCode,
  RefreshCw,
  AlertTriangle,
  CheckCircle2,
  Lock,
  Cpu,
  Plus,
  Trash2,
} from 'lucide-react';
import { api } from '../api/client';
import {
  p10Api,
  type ActorDelegationReceipt,
  type ActorRevocationReceipt,
  type MeshDeviceSurfaceKind,
  type MeshPairingSession,
  type PersonalMeshStatus,
  type SurfaceCapabilityTokenView,
} from '../api/p10Client';
import {
  COVERAGE_LOCAL_STORE_ONLY,
  PROVENANCE_PERSONAL_MESH_ENCRYPTED,
  PROVENANCE_PRIVATE_LOCAL_ONLY,
  SURFACE_CAPABILITY_CEILINGS,
  USABLE_OFFLINE_BADGE,
  formatCoverageBadge,
  type AuxiliarySurfaceKind,
} from '../utils/epistemicBadges';

const SURFACE_ICONS: Record<string, string> = {
  desktop: '💻',
  mobile: '📱',
  web: '🌐',
  cli: '⌨️',
  glasses: '🕶️',
};

function formatRelativeUnix(ts: number | null | undefined): string {
  if (!ts) return 'Not yet synced';
  const diffSec = Math.max(0, Math.floor(Date.now() / 1000) - ts);
  if (diffSec < 60) return 'Just now';
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86_400) return `${Math.floor(diffSec / 3600)}h ago`;
  return new Date(ts * 1000).toLocaleDateString();
}

export function DevicesPage() {
  const [meshStatus, setMeshStatus] = useState<PersonalMeshStatus | null>(null);
  const [surfaceTokens, setSurfaceTokens] = useState<SurfaceCapabilityTokenView[]>([]);
  const [offlineFallback, setOfflineFallback] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState('');
  const [error, setError] = useState<string | null>(null);

  // SPAKE2+ / QR Pairing state
  const [pairLabel, setPairLabel] = useState('Field Mobile Companion');
  const [pairSurfaceKind, setPairSurfaceKind] = useState<MeshDeviceSurfaceKind>('mobile');
  const [pairingSession, setPairingSession] = useState<MeshPairingSession | null>(null);
  const [sasConfirmed, setSasConfirmed] = useState(false);
  const [delegationReceipt, setDelegationReceipt] = useState<ActorDelegationReceipt | null>(null);

  // Epoch rotation state (`ActorRevocation/1`)
  const [revocationReceipt, setRevocationReceipt] = useState<ActorRevocationReceipt | null>(null);

  // Scoped `obc_surface1` token minting state
  const [mintSurfaceKind, setMintSurfaceKind] = useState<AuxiliarySurfaceKind>('extension');
  const [mintLabel, setMintLabel] = useState('');
  const [mintOrigin, setMintOrigin] = useState('');
  const [mintedHandle, setMintedHandle] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    setBusy('refresh');
    try {
      const [meshRes, tokRes] = await Promise.all([
        p10Api.getMeshStatus(),
        p10Api.listSurfaceTokens(),
        // Best-effort legacy device endpoint check; never blocks USABLE_OFFLINE
        api.syncStatus().catch(() => null),
      ]);
      setMeshStatus(meshRes.data);
      setSurfaceTokens(tokRes.data.items);
      setOfflineFallback(meshRes.offlineFallback || tokRes.offlineFallback);
      setError(null);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Unable to refresh Personal Mesh status');
    } finally {
      setBusy('');
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const handleStartPairing = async () => {
    if (!pairLabel.trim()) return;
    setBusy('pair-start');
    setError(null);
    setDelegationReceipt(null);
    setSasConfirmed(false);
    try {
      const res = await p10Api.startMeshPairing({
        device_label: pairLabel.trim(),
        surface_kind: pairSurfaceKind,
      });
      setPairingSession(res.data);
      setOfflineFallback(prev => prev || res.offlineFallback);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to initiate SPAKE2+ pairing session');
    } finally {
      setBusy('');
    }
  };

  const handleConfirmPairing = async () => {
    if (!pairingSession || !sasConfirmed) return;
    setBusy('pair-confirm');
    setError(null);
    try {
      const res = await p10Api.confirmMeshPairing({
        session_id: pairingSession.session_id,
        sas_confirmed: sasConfirmed,
        sas_digits: pairingSession.sas_digits,
      });
      setDelegationReceipt(res.data);
      setPairingSession(null);
      setSasConfirmed(false);
      const refreshed = await p10Api.getMeshStatus();
      setMeshStatus(refreshed.data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to confirm pairing delegation');
    } finally {
      setBusy('');
    }
  };

  const handleToggleVaultSync = async () => {
    if (!meshStatus) return;
    setBusy('sync-toggle');
    setError(null);
    try {
      const nextEnabled = !meshStatus.vault_sync_enabled;
      await p10Api.syncPersonalMesh({
        vault_sync_enabled: nextEnabled,
        trigger_reconcile: nextEnabled,
      });
      const refreshed = await p10Api.getMeshStatus();
      setMeshStatus(refreshed.data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to update PersonalMeshEpochDek sync state');
    } finally {
      setBusy('');
    }
  };

  const handleRevokeDevice = async (deviceId: string) => {
    setBusy(`revoke-${deviceId}`);
    setError(null);
    try {
      const res = await p10Api.revokeMeshDevice({
        device_id: deviceId,
        reason: 'compromised',
      });
      setRevocationReceipt(res.data);
      const refreshed = await p10Api.getMeshStatus();
      setMeshStatus(refreshed.data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to issue ActorRevocation/1');
    } finally {
      setBusy('');
    }
  };

  const handleMintToken = async () => {
    setBusy('mint-token');
    setError(null);
    setMintedHandle(null);
    try {
      const defaultOrigin =
        mintSurfaceKind === 'extension'
          ? 'chrome-extension://onebrain-capture'
          : mintSurfaceKind === 'bot'
            ? 'bot-dmz://channel-adapter'
            : 'obp/companion-hud/1://paired-glasses';
      const res = await p10Api.mintSurfaceToken({
        surface_kind: mintSurfaceKind,
        label: mintLabel.trim() || `${mintSurfaceKind} scoped adapter`,
        bound_channel_or_origin: mintOrigin.trim() || defaultOrigin,
      });
      setMintedHandle(res.data.issued_capability_handle);
      setMintLabel('');
      setMintOrigin('');
      const listed = await p10Api.listSurfaceTokens();
      setSurfaceTokens(listed.data.items);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to mint obc_surface1 token');
    } finally {
      setBusy('');
    }
  };

  const handleRevokeToken = async (tokenId: string) => {
    setBusy(`revoke-tok-${tokenId}`);
    setError(null);
    try {
      await p10Api.revokeSurfaceToken(tokenId);
      const listed = await p10Api.listSurfaceTokens();
      setSurfaceTokens(listed.data.items);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to revoke obc_surface1 token');
    } finally {
      setBusy('');
    }
  };

  if (loading || !meshStatus) {
    return (
      <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}>
        <div className="spinner spinner-lg" />
      </div>
    );
  }

  const coverageBadge = formatCoverageBadge({
    coverage: meshStatus.reachable_mesh_peers > 0 ? 'partial' : 'local_only',
    reachablePeers: meshStatus.reachable_mesh_peers,
  });
  const vaultProvenanceBadge = meshStatus.vault_sync_enabled
    ? PROVENANCE_PERSONAL_MESH_ENCRYPTED
    : PROVENANCE_PRIVATE_LOCAL_ONLY;

  return (
    <div className="page">
      {/* ─── Header & Canonical Epistemic Badges ─────────────────────────── */}
      <div className="page-header">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', flexWrap: 'wrap', gap: 12 }}>
          <div>
            <h1>Personal Mesh &amp; Scoped Surface Adapters</h1>
            <p>
              Zero-key-reuse multi-device delegation (<code>obp/personal-mesh/1</code>) &amp; least-privilege{' '}
              <code>obc_surface1</code> capability ceilings
            </p>
          </div>
          <button className="btn btn-sm" onClick={() => void loadData()} disabled={!!busy}>
            <RefreshCw size={14} /> Refresh Mesh
          </button>
        </div>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 12 }}>
          <span className="badge badge-green" style={{ textTransform: 'none' }}>
            {USABLE_OFFLINE_BADGE}
          </span>
          <span className="badge badge-cyan" style={{ textTransform: 'none' }}>
            {vaultProvenanceBadge}
          </span>
          <span className="badge badge-violet" style={{ textTransform: 'none' }}>
            {coverageBadge}
          </span>
          <span className="badge badge-amber" style={{ textTransform: 'none' }}>
            mesh_epoch: {meshStatus.mesh_epoch}
          </span>
          {offlineFallback && (
            <span className="badge" style={{ textTransform: 'none', border: '1px solid var(--ob-glass-border)' }}>
              {COVERAGE_LOCAL_STORE_ONLY} · Local offline control plane active
            </span>
          )}
        </div>
      </div>

      {error && (
        <div
          role="alert"
          className="glass-card"
          style={{
            marginBottom: 'var(--ob-gap-md)',
            borderColor: 'rgba(239, 68, 68, 0.4)',
            color: 'var(--ob-error)',
            fontSize: '0.85rem',
          }}
        >
          <AlertTriangle size={16} style={{ verticalAlign: 'middle', marginRight: 8 }} />
          {error}
        </div>
      )}

      {/* ─── 1. Four-Principal Separation Card (`NEG-P10-001`) ───────────── */}
      <section
        className="glass-card"
        aria-labelledby="p10-principals-heading"
        style={{ marginBottom: 'var(--ob-gap-lg)', borderColor: 'rgba(6, 182, 212, 0.35)' }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 10 }}>
          <KeyRound size={18} style={{ color: 'var(--ob-accent-light)' }} />
          <h2 id="p10-principals-heading" style={{ fontSize: '1.05rem', fontWeight: 600 }}>
            Four-Principal Cryptographic Separation (NEG-P10-001)
          </h2>
        </div>
        <p style={{ fontSize: '0.84rem', color: 'var(--ob-text-secondary)', marginBottom: 14 }}>
          <strong>Zero-Key-Reuse Law:</strong> Private signing keys (<code>FeedEventSigner</code> /{' '}
          <code>FeedId</code>) are <strong>NEVER</strong> shared across devices. Each installation maintains a
          single-writer append-only feed log so offline partitions cannot cause accidental multi-device feed
          equivocation.
        </p>

        <div className="grid-2" style={{ gap: 12 }}>
          {[
            {
              role: 'ActorId (Root Authority)',
              id: meshStatus.principals.actor_id,
              desc: '32-byte human root anchor (recovery phrase / hardware passkey)',
            },
            {
              role: 'DeviceId (Hardware Installation)',
              id: meshStatus.principals.device_id,
              desc: '32-byte per-installation hardware-bound key (OS Keychain / Enclave)',
            },
            {
              role: 'FeedId (FeedEventSigner · Single-Writer)',
              id: meshStatus.principals.feed_id,
              desc: 'Per-device append-only event log signer — NEVER exported or shared',
            },
            {
              role: 'NodeId (SessionIdentitySigner)',
              id: meshStatus.principals.node_id,
              desc: 'Per-endpoint NET-001 mutually authenticated QUIC transport identity',
            },
          ].map(item => (
            <div
              key={item.role}
              style={{
                padding: '12px 14px',
                borderRadius: 'var(--ob-radius-md)',
                background: 'var(--ob-bg-secondary)',
                border: '1px solid var(--ob-glass-border)',
              }}
            >
              <div style={{ fontSize: '0.78rem', fontWeight: 600, color: 'var(--ob-accent-light)' }}>
                {item.role}
              </div>
              <div
                className="mono"
                style={{
                  fontSize: '0.74rem',
                  color: 'var(--ob-text-primary)',
                  marginTop: 4,
                  overflowWrap: 'anywhere',
                }}
              >
                {item.id}
              </div>
              <div style={{ fontSize: '0.74rem', color: 'var(--ob-text-tertiary)', marginTop: 4 }}>
                {item.desc}
              </div>
            </div>
          ))}
        </div>
      </section>

      {/* ─── 2. Out-of-Band SPAKE2+ / QR Device Pairing Wizard ───────────── */}
      <section
        className="glass-card"
        aria-labelledby="p10-pairing-heading"
        style={{ marginBottom: 'var(--ob-gap-lg)' }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 10 }}>
          <QrCode size={18} style={{ color: 'var(--ob-violet)' }} />
          <h2 id="p10-pairing-heading" style={{ fontSize: '1.05rem', fontWeight: 600 }}>
            SPAKE2+ / QR Out-of-Band Device Pairing (PersonalMeshPairV1)
          </h2>
        </div>
        <p style={{ fontSize: '0.84rem', color: 'var(--ob-text-secondary)', marginBottom: 14 }}>
          Enrolls a sibling device over an authenticated <code>NET-001</code> transcript and issues an
          attenuated <code>ActorDelegation/1</code> binding only after you verify the 6-digit and 4-word Short
          Authentication String (SAS) on both screens.
        </p>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: 12, alignItems: 'end' }}>
          <div>
            <label htmlFor="mesh-pair-label" style={{ display: 'block', fontSize: '0.78rem', color: 'var(--ob-text-secondary)', marginBottom: 4 }}>
              New device label
            </label>
            <input
              id="mesh-pair-label"
              className="input"
              value={pairLabel}
              onChange={e => setPairLabel(e.target.value)}
              placeholder="e.g. Field Mobile Companion"
            />
          </div>
          <div>
            <label htmlFor="mesh-pair-surface" style={{ display: 'block', fontSize: '0.78rem', color: 'var(--ob-text-secondary)', marginBottom: 4 }}>
              Target surface kind
            </label>
            <select
              id="mesh-pair-surface"
              className="input"
              value={pairSurfaceKind}
              onChange={e => setPairSurfaceKind(e.target.value as MeshDeviceSurfaceKind)}
            >
              <option value="mobile">Mobile (onebrain-mobile)</option>
              <option value="desktop">Desktop (onebrain-desktop)</option>
              <option value="cli">CLI (onebrain-cli)</option>
              <option value="glasses">Smart Glasses (onebrain-glasses)</option>
            </select>
          </div>
          <div>
            <button
              className="btn btn-primary"
              onClick={() => void handleStartPairing()}
              disabled={!!busy || !pairLabel.trim()}
            >
              <QrCode size={15} /> Start SPAKE2+ / QR Pairing
            </button>
          </div>
        </div>

        {pairingSession && (
          <div
            aria-label="Active SPAKE2+ pairing ceremony"
            style={{
              marginTop: 16,
              padding: 16,
              borderRadius: 'var(--ob-radius-md)',
              background: 'rgba(139, 92, 246, 0.08)',
              border: '1px solid rgba(139, 92, 246, 0.35)',
            }}
          >
            <h3 style={{ fontSize: '0.95rem', fontWeight: 600, marginBottom: 8 }}>
              Verify Short Authentication String (SAS) Before Delegation
            </h3>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: 12, marginBottom: 12 }}>
              <div style={{ padding: 12, background: 'var(--ob-bg-secondary)', borderRadius: 8 }}>
                <div style={{ fontSize: '0.72rem', color: 'var(--ob-text-tertiary)' }}>Out-of-Band QR / URI Code</div>
                <div className="mono" style={{ fontSize: '0.74rem', marginTop: 4, overflowWrap: 'anywhere', color: 'var(--ob-accent-light)' }}>
                  {pairingSession.qr_payload}
                </div>
              </div>
              <div style={{ padding: 12, background: 'var(--ob-bg-secondary)', borderRadius: 8 }}>
                <div style={{ fontSize: '0.72rem', color: 'var(--ob-text-tertiary)' }}>6-Digit &amp; 4-Word SAS</div>
                <div className="mono" style={{ fontSize: '1.15rem', fontWeight: 700, color: '#fbbf24', marginTop: 4 }}>
                  {pairingSession.sas_digits}
                </div>
                <div className="mono" style={{ fontSize: '0.82rem', color: 'var(--ob-text-primary)', marginTop: 4 }}>
                  {pairingSession.sas_words.join(' · ')}
                </div>
              </div>
            </div>

            <div className="mono" style={{ fontSize: '0.72rem', color: 'var(--ob-text-secondary)', marginBottom: 12, overflowWrap: 'anywhere' }}>
              Candidate DeviceId: {pairingSession.candidate_device_id} · Candidate FeedId:{' '}
              {pairingSession.candidate_feed_id} · Target mesh_epoch: {pairingSession.target_mesh_epoch}
            </div>

            <label
              htmlFor="mesh-sas-confirm"
              style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: '0.84rem', cursor: 'pointer', marginBottom: 12 }}
            >
              <input
                id="mesh-sas-confirm"
                type="checkbox"
                checked={sasConfirmed}
                onChange={e => setSasConfirmed(e.target.checked)}
              />
              I verified that both the 6-digit SAS ({pairingSession.sas_digits}) and 4-word SAS (
              {pairingSession.sas_words.join(' · ')}) match on both screens
            </label>

            <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
              <button
                className="btn btn-primary"
                disabled={!sasConfirmed || !!busy}
                onClick={() => void handleConfirmPairing()}
              >
                <CheckCircle2 size={15} /> Confirm SAS &amp; Issue ActorDelegation/1
              </button>
              <button
                className="btn"
                disabled={!!busy}
                onClick={() => {
                  setPairingSession(null);
                  setSasConfirmed(false);
                }}
              >
                Abort Pairing
              </button>
            </div>
          </div>
        )}

        {delegationReceipt && (
          <div
            role="status"
            style={{
              marginTop: 14,
              padding: 12,
              borderRadius: 'var(--ob-radius-md)',
              background: 'rgba(16, 185, 129, 0.1)',
              border: '1px solid rgba(16, 185, 129, 0.35)',
              fontSize: '0.82rem',
            }}
          >
            <strong>Issued {delegationReceipt.delegation_kind}</strong> (mesh_epoch{' '}
            {delegationReceipt.mesh_epoch}) · Event CID:{' '}
            <span className="mono">{delegationReceipt.delegation_event_cid.slice(0, 20)}…</span> · New FeedId:{' '}
            <span className="mono">{delegationReceipt.new_feed_id.slice(0, 16)}…</span> · Private signing keys
            transmitted: <code>{String(delegationReceipt.private_keys_transmitted)}</code>
          </div>
        )}
      </section>

      {/* ─── 3. Active Mesh Devices, Epoch Sync & `ActorRevocation/1` (`FW-10`) ─ */}
      <section
        className="glass-card"
        aria-labelledby="p10-mesh-devices-heading"
        style={{ marginBottom: 'var(--ob-gap-lg)' }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 12, marginBottom: 12 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Lock size={18} style={{ color: 'var(--ob-success)' }} />
            <h2 id="p10-mesh-devices-heading" style={{ fontSize: '1.05rem', fontWeight: 600 }}>
              Active Personal Mesh Devices &amp; Epoch Rotation (FW-10)
            </h2>
          </div>
          <label
            htmlFor="mesh-vault-sync-toggle"
            style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: '0.84rem', cursor: 'pointer' }}
          >
            <input
              id="mesh-vault-sync-toggle"
              type="checkbox"
              checked={meshStatus.vault_sync_enabled}
              disabled={!!busy}
              onChange={() => void handleToggleVaultSync()}
            />
            Sync opted-in Vault items under <code>PersonalMeshEpochDek(epoch={meshStatus.mesh_epoch})</code>
          </label>
        </div>

        <p style={{ fontSize: '0.82rem', color: 'var(--ob-text-secondary)', marginBottom: 12 }}>
          Encryption: <code>{meshStatus.epoch_dek_algorithm}</code> wrapped individually via{' '}
          <code>{meshStatus.key_wrap_algorithm}</code> to active non-revoked device HPKE keys · Last sync:{' '}
          {formatRelativeUnix(meshStatus.last_sync_unix_s)} · Never synced:{' '}
          {meshStatus.excluded_never_synced.join(', ')}.
        </p>

        {revocationReceipt && (
          <div
            role="status"
            aria-label="ActorRevocation epoch rotation notice"
            style={{
              marginBottom: 14,
              padding: 14,
              borderRadius: 'var(--ob-radius-md)',
              background: 'rgba(245, 158, 11, 0.12)',
              border: '1px solid rgba(245, 158, 11, 0.4)',
              fontSize: '0.83rem',
            }}
          >
            <strong>
              {revocationReceipt.revocation_kind} committed — Forward-Secrecy Epoch Rotated:{' '}
              <code>
                mesh_epoch {revocationReceipt.previous_mesh_epoch} → {revocationReceipt.new_mesh_epoch}
              </code>
            </strong>
            <div style={{ marginTop: 4, color: 'var(--ob-text-secondary)' }}>
              Revoked DeviceId <span className="mono">{revocationReceipt.revoked_device_id.slice(0, 16)}…</span>{' '}
              immediately excluded; fresh <code>PersonalMeshEpochDek({revocationReceipt.new_mesh_epoch})</code>{' '}
              wrapped strictly to {revocationReceipt.remaining_active_devices} remaining active device(s).
              Historical public FeedEvents prior to revocation frontier remain verifiable (FEED-002).
            </div>
          </div>
        )}

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(310px, 1fr))', gap: 12 }}>
          {meshStatus.devices.map(dev => {
            const isRevoked = dev.status === 'revoked';
            const icon = SURFACE_ICONS[dev.surface_kind] ?? '📟';
            return (
              <article
                key={dev.device_id}
                style={{
                  padding: 16,
                  borderRadius: 'var(--ob-radius-md)',
                  background: 'var(--ob-bg-secondary)',
                  border: `1px solid ${isRevoked ? 'rgba(239, 68, 68, 0.35)' : 'var(--ob-glass-border)'}`,
                  opacity: isRevoked ? 0.72 : 1,
                }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    <span style={{ fontSize: '1.4rem' }}>{icon}</span>
                    <div>
                      <div style={{ fontWeight: 600, fontSize: '0.92rem' }}>{dev.label}</div>
                      <div style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)' }}>
                        {dev.surface_kind} · mesh_epoch {dev.mesh_epoch}
                      </div>
                    </div>
                  </div>
                  <span className={`badge ${isRevoked ? 'badge-amber' : 'badge-green'}`}>
                    {isRevoked ? 'revoked' : dev.is_current_device ? 'this host' : 'active'}
                  </span>
                </div>

                <div className="mono" style={{ fontSize: '0.72rem', color: 'var(--ob-text-secondary)', marginTop: 10, overflowWrap: 'anywhere' }}>
                  <div>DeviceId: {dev.device_id.slice(0, 20)}…</div>
                  <div>FeedId (single-writer): {dev.feed_id.slice(0, 20)}…</div>
                  <div>NodeId: {dev.node_id.slice(0, 20)}…</div>
                </div>

                {!dev.is_current_device && !isRevoked && (
                  <button
                    className="btn btn-sm"
                    style={{ marginTop: 12, borderColor: 'rgba(239, 68, 68, 0.45)', color: '#fca5a5' }}
                    disabled={!!busy}
                    onClick={() => void handleRevokeDevice(dev.device_id)}
                  >
                    <Trash2 size={13} /> Revoke Lost/Compromised Device
                  </button>
                )}
              </article>
            );
          })}
        </div>
      </section>

      {/* ─── 4. Scoped Surface Capability Manager (`obc_surface1`) ───────── */}
      <section
        className="glass-card"
        aria-labelledby="p10-surfaces-heading"
        style={{ marginBottom: 'var(--ob-gap-lg)' }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 10 }}>
          <Shield size={18} style={{ color: 'var(--ob-accent)' }} />
          <h2 id="p10-surfaces-heading" style={{ fontSize: '1.05rem', fontWeight: 600 }}>
            Scoped Surface Capability Manager (<code>obc_surface1</code>)
          </h2>
        </div>
        <p style={{ fontSize: '0.84rem', color: 'var(--ob-text-secondary)', marginBottom: 14 }}>
          Mints and revokes BLAKE3-MACed <code>obc_surface1</code> capability handles bound to auxiliary
          surface identity and hard protocol ceilings (<code>NEG-P10-002</code>, <code>NEG-P10-003</code>).
        </p>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: 12, alignItems: 'end', marginBottom: 16 }}>
          <div>
            <label htmlFor="surface-kind-select" style={{ display: 'block', fontSize: '0.78rem', color: 'var(--ob-text-secondary)', marginBottom: 4 }}>
              Auxiliary surface
            </label>
            <select
              id="surface-kind-select"
              className="input"
              value={mintSurfaceKind}
              onChange={e => setMintSurfaceKind(e.target.value as AuxiliarySurfaceKind)}
            >
              <option value="extension">Extension (onebrain-extension)</option>
              <option value="bot">Bot (onebrain-bot · PUBLIC-only + Quarantine)</option>
              <option value="glasses">Glasses (onebrain-glasses · &lt;= 40ms doff-lock)</option>
            </select>
          </div>
          <div>
            <label htmlFor="surface-label-input" style={{ display: 'block', fontSize: '0.78rem', color: 'var(--ob-text-secondary)', marginBottom: 4 }}>
              Adapter label
            </label>
            <input
              id="surface-label-input"
              className="input"
              value={mintLabel}
              onChange={e => setMintLabel(e.target.value)}
              placeholder="e.g. Research Browser Capture"
            />
          </div>
          <div>
            <label htmlFor="surface-origin-input" style={{ display: 'block', fontSize: '0.78rem', color: 'var(--ob-text-secondary)', marginBottom: 4 }}>
              Pinned origin / channel binding
            </label>
            <input
              id="surface-origin-input"
              className="input"
              value={mintOrigin}
              onChange={e => setMintOrigin(e.target.value)}
              placeholder="chrome-extension://... or bot-dmz://..."
            />
          </div>
          <div>
            <button className="btn btn-primary" disabled={!!busy} onClick={() => void handleMintToken()}>
              <Plus size={15} /> Mint obc_surface1 Token
            </button>
          </div>
        </div>

        {mintedHandle && (
          <div
            role="status"
            style={{
              marginBottom: 16,
              padding: 12,
              borderRadius: 'var(--ob-radius-md)',
              background: 'rgba(6, 182, 212, 0.1)',
              border: '1px solid rgba(6, 182, 212, 0.35)',
              fontSize: '0.82rem',
            }}
          >
            Minted least-privilege capability handle: <code className="mono">{mintedHandle}</code>
          </div>
        )}

        {/* Active Surface Tokens */}
        <div style={{ display: 'grid', gap: 10, marginBottom: 20 }}>
          {surfaceTokens.map(tok => (
            <article
              key={tok.token_id}
              style={{
                padding: 14,
                borderRadius: 'var(--ob-radius-md)',
                background: 'var(--ob-bg-secondary)',
                border: '1px solid var(--ob-glass-border)',
                opacity: tok.revoked ? 0.65 : 1,
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 8 }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                  <Cpu size={15} style={{ color: 'var(--ob-accent-light)' }} />
                  <strong>{tok.label}</strong>
                  <span className="badge badge-cyan" style={{ textTransform: 'none' }}>
                    {tok.surface_kind === 'extension'
                      ? 'Extension · Draft Capture Only'
                      : tok.surface_kind === 'bot'
                        ? 'Bot · PUBLIC-only + Quarantine'
                        : 'Glasses · <= 40ms doff-lock + two-step physical confirm'}
                  </span>
                  <span className="mono" style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)' }}>
                    {tok.token_preview}
                  </span>
                </div>
                {tok.revoked ? (
                  <span className="badge badge-amber">Revoked</span>
                ) : (
                  <button
                    className="btn btn-sm"
                    disabled={!!busy}
                    onClick={() => void handleRevokeToken(tok.token_id)}
                    aria-label={`Revoke surface token ${tok.label}`}
                  >
                    Revoke Token
                  </button>
                )}
              </div>
              <div style={{ fontSize: '0.78rem', color: 'var(--ob-text-secondary)', marginTop: 8 }}>
                Bound channel: <code>{tok.bound_channel_or_origin}</code> · PrivateVault scope:{' '}
                <strong>{tok.private_vault_scope}</strong> · Step-2 PUBLIC confirm:{' '}
                <code>{tok.step2_public_confirm}</code>
                {tok.doff_lock_max_ms !== null && (
                  <> · Wear-sensor doff-lock: <code>&lt;= {tok.doff_lock_max_ms}ms</code></>
                )}
                {tok.quarantine_inbound && <> · Inbound submissions: <code>QuarantineRecord</code></>}
              </div>
              <div style={{ fontSize: '0.74rem', color: 'var(--ob-text-tertiary)', marginTop: 4 }}>
                Hard prohibitions: {tok.hard_prohibitions.join(' · ')}
              </div>
            </article>
          ))}
        </div>

        {/* Hard Capability Ceiling Reference Matrix */}
        <h3 style={{ fontSize: '0.92rem', fontWeight: 600, marginBottom: 8 }}>
          Canonical Per-Surface Capability Ceiling Matrix (P10 §4)
        </h3>
        <div style={{ overflowX: 'auto' }}>
          <table className="data-table">
            <thead>
              <tr>
                <th>Surface</th>
                <th>Transport</th>
                <th>PrivateVault Read</th>
                <th>obm1 Admin / Archive</th>
                <th>Step-2 PUBLIC Confirm</th>
                <th>Somatic P9 Access</th>
              </tr>
            </thead>
            <tbody>
              {SURFACE_CAPABILITY_CEILINGS.map(row => (
                <tr key={row.surfaceId}>
                  <td>
                    <span className="mono">{row.surfaceId}</span>
                  </td>
                  <td>{row.transport}</td>
                  <td>{row.privateVaultRead}</td>
                  <td>{row.obm1AdminArchive}</td>
                  <td>{row.step2PublicConfirm}</td>
                  <td>{row.somaticP9Access}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}
