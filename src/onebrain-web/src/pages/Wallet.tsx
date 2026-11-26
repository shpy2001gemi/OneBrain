import { useEffect, useState } from 'react';
import { Coins, ArrowUpRight, ArrowDownRight, Shield, Clock, AlertTriangle } from 'lucide-react';
import { api } from '../api/client';
import { p10Api, type ObtVNextStatusView } from '../api/p10Client';
import type { WalletInfo, WalletTransaction } from '../api/types';
import { formatObtSigned, formatDateFull as formatWalletTimestamp } from '../utils/format';
import {
  COVERAGE_LOCAL_STORE_ONLY,
  ECONOMIC_COMPATIBILITY_BADGE,
  PROVISIONAL_PARTITION_SETTLEMENT_BADGE,
  USABLE_OFFLINE_BADGE,
} from '../utils/epistemicBadges';

const OFFLINE_SIMULATED_WALLET: WalletInfo = {
  economic_status: 'simulated_non_economic',
  limitations: ['usable_offline_simulated_projection', 'no_account_chain_settlement_connected'],
  balance: 0,
  chain_length: 0,
  tier: 'LocalNode',
  multiplier: 1,
  total_earned: 0,
  total_spent: 0,
  staked: 0,
  pending_unstake: 0,
  streams: { owner: 0, encoder: 0, verifier: 0, storage: 0 },
  rate_used: 0,
  rate_max: 100,
};

export function WalletPage() {
  const [wallet, setWallet] = useState<WalletInfo>(OFFLINE_SIMULATED_WALLET);
  const [txns, setTxns] = useState<WalletTransaction[]>([]);
  const [obtStatus, setObtStatus] = useState<ObtVNextStatusView | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      api.getWallet().then(setWallet).catch(() => setWallet(OFFLINE_SIMULATED_WALLET)),
      api.getWalletHistory(20).then(setTxns).catch(() => setTxns([])),
      p10Api.getObtStatus().then(res => setObtStatus(res.data)),
    ]).finally(() => setLoading(false));
  }, []);

  const formatObt = formatObtSigned;
  const formatDate = formatWalletTimestamp;

  if (loading) {
    return (
      <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}>
        <div className="spinner spinner-lg" />
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-header">
        <h1>OBT Wallet Simulation</h1>
        <p>Non-economic compatibility projection; no AccountChain or monetary settlement is connected</p>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 10 }}>
          <span className="badge badge-amber" style={{ textTransform: 'none' }}>
            {ECONOMIC_COMPATIBILITY_BADGE}
          </span>
          <span className="badge badge-cyan" style={{ textTransform: 'none' }}>
            {COVERAGE_LOCAL_STORE_ONLY}
          </span>
          <span className="badge badge-green" style={{ textTransform: 'none' }}>
            {USABLE_OFFLINE_BADGE}
          </span>
        </div>
      </div>

      <div
        className="glass-card"
        style={{
          borderColor: 'rgba(245,158,11,0.45)',
          color: 'var(--ob-warning)',
          marginBottom: 'var(--ob-gap-lg)',
        }}
      >
        <strong>{ECONOMIC_COMPATIBILITY_BADGE} — SIMULATED / NON-ECONOMIC.</strong> These values are
        derived from local KU count and are not spendable, transferable, settled, or reward-authoritative.
        {wallet.limitations.map(limit => (
          <div key={limit} style={{ marginTop: 4 }}>
            • {limit}
          </div>
        ))}
      </div>

      {/* Read-Only vNext Pillar 5 (`P5`) ShadowEvaluate & Partition-Honest Status Panel */}
      {obtStatus && (
        <section
          className="glass-card"
          aria-labelledby="p5-obt-status-heading"
          style={{ marginBottom: 'var(--ob-gap-lg)', borderColor: 'rgba(139, 92, 246, 0.35)' }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 8, marginBottom: 10 }}>
            <h2 id="p5-obt-status-heading" style={{ fontSize: '1rem', fontWeight: 600 }}>
              vNext Pillar 5 (P5) Read-Only Economy &amp; Settlement Gate
            </h2>
            <span className="badge badge-violet" style={{ textTransform: 'none' }}>
              obt_mode: {obtStatus.obt_mode}
            </span>
          </div>
          <p style={{ fontSize: '0.84rem', color: 'var(--ob-text-secondary)', marginBottom: 12 }}>
            Production ledger flag <code>obt_ledger_v1</code> is <code>{String(obtStatus.obt_ledger_v1_enabled)}</code>.
            Under <code>ShadowEvaluate</code>, post-commit notices evaluate deterministic{' '}
            <code>RewardClaimCandidate</code> records across Track A (<code>D-014</code> accepted encode &amp;
            blind verify work, with evidenced-mismatch symmetry) and Track B (<code>M6B/M7</code> attributed
            benefit outcomes) with zero <code>AccountBlock</code> mutations.
          </p>

          <div className="grid-3" style={{ marginBottom: 12 }}>
            <div style={{ padding: 12, borderRadius: 8, background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)' }}>
              <div style={{ fontSize: '0.74rem', color: 'var(--ob-text-tertiary)' }}>PendingMint Records</div>
              <div style={{ fontSize: '1.1rem', fontWeight: 700, marginTop: 4 }}>
                {obtStatus.pending_mint_records} (PendingMint · spendable={String(obtStatus.pending_mint_spendable)})
              </div>
            </div>
            <div style={{ padding: 12, borderRadius: 8, background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)' }}>
              <div style={{ fontSize: '0.74rem', color: 'var(--ob-text-tertiary)' }}>Shadow Candidates (Track A / B)</div>
              <div style={{ fontSize: '1.1rem', fontWeight: 700, marginTop: 4 }}>
                {obtStatus.track_a_work_claims_shadow} work / {obtStatus.track_b_benefit_claims_shadow} benefit
              </div>
            </div>
            <div style={{ padding: 12, borderRadius: 8, background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)' }}>
              <div style={{ fontSize: '0.74rem', color: 'var(--ob-text-tertiary)' }}>Offline Island Slashing</div>
              <div style={{ fontSize: '1.1rem', fontWeight: 700, color: 'var(--ob-success)', marginTop: 4 }}>
                Zero penalty ({String(obtStatus.offline_penalty_or_slashing)})
              </div>
            </div>
          </div>

          <div
            role="note"
            style={{
              padding: '10px 12px',
              borderRadius: 'var(--ob-radius-sm)',
              background: 'rgba(245, 158, 11, 0.1)',
              border: '1px solid rgba(245, 158, 11, 0.35)',
              fontSize: '0.8rem',
              color: '#fde68a',
            }}
          >
            <AlertTriangle size={14} style={{ verticalAlign: 'middle', marginRight: 6 }} />
            Partition settlement label ({obtStatus.settlement_state}):{' '}
            <strong>{PROVISIONAL_PARTITION_SETTLEMENT_BADGE}</strong>
          </div>
        </section>
      )}

      {/* Balance + Tier */}
      <div className="grid-4" style={{ marginBottom: 'var(--ob-gap-lg)' }}>
        <div className="glass-card accent-glow stat-card animate-in">
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
            <span className="stat-label">Simulated balance</span>
            <Coins size={18} style={{ color: 'var(--ob-warning)', opacity: 0.7 }} />
          </div>
          <span className="stat-value">{formatObt(wallet.balance)}</span>
          <span className="stat-sub">non-economic units ({wallet.balance} milli-units)</span>
        </div>
        <div className="glass-card stat-card animate-in" style={{ animationDelay: '80ms' }}>
          <span className="stat-label">Tier</span>
          <span className="stat-value">{wallet.tier}</span>
          <span className="stat-sub">{wallet.multiplier}x multiplier</span>
        </div>
        <div className="glass-card stat-card animate-in" style={{ animationDelay: '160ms' }}>
          <span className="stat-label">Derived credit</span>
          <span className="stat-value">{formatObt(wallet.total_earned)}</span>
          <span className="stat-sub">lifetime</span>
        </div>
        <div className="glass-card stat-card animate-in" style={{ animationDelay: '240ms' }}>
          <span className="stat-label">Simulated records</span>
          <span className="stat-value">{wallet.chain_length}</span>
          <span className="stat-sub">blocks</span>
        </div>
      </div>

      {/* Earnings Streams + Rate Limit */}
      <div className="grid-3" style={{ gridTemplateColumns: '2fr 1fr', marginBottom: 'var(--ob-gap-lg)' }}>
        <div className="glass-card animate-in" style={{ animationDelay: '300ms' }}>
          <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)' }}>
            Simulated stream projection (no rewards)
          </h3>
          {[
            { label: 'R1: Owner (PoMV)', value: wallet.streams.owner, pct: 40, color: '#06b6d4' },
            { label: 'R2: Encoder', value: wallet.streams.encoder, pct: 25, color: '#8b5cf6' },
            { label: 'R3: Verifier', value: wallet.streams.verifier, pct: 15, color: '#10b981' },
            { label: 'R4: Storage', value: wallet.streams.storage, pct: 20, color: '#f59e0b' },
          ].map(s => (
            <div key={s.label} style={{ marginBottom: 'var(--ob-gap-md)' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                <span style={{ fontSize: '0.85rem' }}>
                  {s.label} ({s.pct}%)
                </span>
                <span style={{ fontWeight: 600, color: s.color }}>{formatObt(s.value)} OBT</span>
              </div>
              <div className="progress-bar" style={{ height: 6 }}>
                <div
                  className="fill"
                  style={{
                    width: wallet.total_earned > 0 ? `${(s.value / wallet.total_earned) * 100}%` : '0%',
                    background: s.color,
                  }}
                />
              </div>
            </div>
          ))}
        </div>

        <div className="glass-card animate-in" style={{ animationDelay: '400ms' }}>
          <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)', display: 'flex', alignItems: 'center', gap: 8 }}>
            <Shield size={18} style={{ color: 'var(--ob-accent)' }} /> Rate Limit
          </h3>
          <div style={{ textAlign: 'center', marginBottom: 'var(--ob-gap-md)' }}>
            <div style={{ fontSize: '2rem', fontWeight: 700 }}>
              {wallet.rate_used} / {wallet.rate_max}
            </div>
            <p style={{ fontSize: '0.82rem', color: 'var(--ob-text-secondary)' }}>encodes this period</p>
          </div>
          <div className="progress-bar" style={{ height: 8 }}>
            <div
              className="fill"
              style={{
                width: wallet.rate_max > 0 ? `${(wallet.rate_used / wallet.rate_max) * 100}%` : '0%',
              }}
            />
          </div>
        </div>
      </div>

      {/* Economic actions are fenced while the wallet is simulated. */}
      <div className="glass-card animate-in" style={{ animationDelay: '450ms', marginBottom: 'var(--ob-gap-lg)' }}>
        <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)' }}>
          Economic actions disabled
        </h3>
        <p style={{ fontSize: '0.85rem', color: 'var(--ob-text-secondary)' }}>
          Stake and unstake are fenced until a production ledger, validation, and finality path passes the M7 exit gate.
        </p>
      </div>

      {/* Transaction History */}
      <div className="glass-card animate-in" style={{ animationDelay: '500ms' }}>
        <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)', display: 'flex', alignItems: 'center', gap: 8 }}>
          <Clock size={18} style={{ color: 'var(--ob-violet)' }} /> Simulated Activity History
        </h3>
        {txns.length === 0 ? (
          <div className="empty-state">
            <Coins size={40} />
            <p>No transactions yet</p>
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Type</th>
                <th>Amount</th>
                <th>Detail</th>
                <th>Status</th>
                <th>Date</th>
              </tr>
            </thead>
            <tbody>
              {txns.map((tx, i) => (
                <tr key={i}>
                  <td>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      {tx.amount >= 0 ? (
                        <ArrowDownRight size={14} style={{ color: 'var(--ob-success)' }} />
                      ) : (
                        <ArrowUpRight size={14} style={{ color: 'var(--ob-error)' }} />
                      )}
                      <span className={`badge ${tx.block_type === 'SimulatedMint' ? 'badge-amber' : tx.block_type === 'Send' ? 'badge-amber' : 'badge-cyan'}`}>
                        {tx.block_type}
                      </span>
                    </div>
                  </td>
                  <td style={{ fontWeight: 600, color: tx.amount >= 0 ? 'var(--ob-success)' : 'var(--ob-error)' }}>
                    {tx.amount >= 0 ? '+' : ''}
                    {formatObt(tx.amount)}
                  </td>
                  <td style={{ fontSize: '0.82rem', color: 'var(--ob-text-secondary)', maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {tx.detail}
                  </td>
                  <td>
                    <span className={`badge ${tx.confirmation === 'Settled' ? 'badge-green' : 'badge-amber'}`}>
                      {tx.confirmation}
                    </span>
                  </td>
                  <td style={{ fontSize: '0.78rem', color: 'var(--ob-text-tertiary)' }}>{formatDate(tx.timestamp)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
