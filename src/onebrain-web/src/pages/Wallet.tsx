import { useEffect, useState } from 'react';
import { Coins, ArrowUpRight, ArrowDownRight, Shield, Clock, Lock, Unlock } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import type { WalletInfo, WalletTransaction } from '../api/types';

export function WalletPage() {
  const [wallet, setWallet] = useState<WalletInfo | null>(null);
  const [txns, setTxns] = useState<WalletTransaction[]>([]);
  const [loading, setLoading] = useState(true);
  const [stakeAmount, setStakeAmount] = useState('');
  const { t } = useTranslation();

  useEffect(() => {
    Promise.all([
      api.getWallet().then(setWallet),
      api.getWalletHistory(20).then(setTxns),
    ]).finally(() => setLoading(false));
  }, []);

  const formatObt = (milli: number) => {
    const sign = milli < 0 ? '-' : '';
    const obt = Math.abs(milli) / 1000;
    const formatted = obt >= 1000 ? `${(obt / 1000).toFixed(2)}K` : obt.toFixed(1);
    return `${sign}${formatted}`;
  };

  const formatDate = (ts: number) => new Date(ts * 1000).toLocaleString();

  if (loading) {
    return <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}>
      <div className="spinner spinner-lg" />
    </div>;
  }

  return (
    <div className="page">
      <div className="page-header">
        <h1>OBT Wallet</h1>
        <p>OneBrain Token balance and transaction history</p>
      </div>

      {wallet && (
        <>
          {/* Balance + Tier */}
          <div className="grid-4" style={{ marginBottom: 'var(--ob-gap-lg)' }}>
            <div className="glass-card accent-glow stat-card animate-in">
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                <span className="stat-label">Balance</span>
                <Coins size={18} style={{ color: 'var(--ob-warning)', opacity: 0.7 }} />
              </div>
              <span className="stat-value">{formatObt(wallet.balance)}</span>
              <span className="stat-sub">OBT (milliOBT: {wallet.balance})</span>
            </div>
            <div className="glass-card stat-card animate-in" style={{ animationDelay: '80ms' }}>
              <span className="stat-label">Tier</span>
              <span className="stat-value">{wallet.tier}</span>
              <span className="stat-sub">{wallet.multiplier}x multiplier</span>
            </div>
            <div className="glass-card stat-card animate-in" style={{ animationDelay: '160ms' }}>
              <span className="stat-label">Total Earned</span>
              <span className="stat-value">{formatObt(wallet.total_earned)}</span>
              <span className="stat-sub">lifetime</span>
            </div>
            <div className="glass-card stat-card animate-in" style={{ animationDelay: '240ms' }}>
              <span className="stat-label">Chain Length</span>
              <span className="stat-value">{wallet.chain_length}</span>
              <span className="stat-sub">blocks</span>
            </div>
          </div>

          {/* Earnings Streams + Rate Limit */}
          <div className="grid-3" style={{ gridTemplateColumns: '2fr 1fr', marginBottom: 'var(--ob-gap-lg)' }}>
            <div className="glass-card animate-in" style={{ animationDelay: '300ms' }}>
              <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)' }}>
                Earnings Streams (4-Revenue Model)
              </h3>
              {[
                { label: 'R1: Owner (PoMV)', value: wallet.streams.owner, pct: 40, color: '#06b6d4' },
                { label: 'R2: Encoder', value: wallet.streams.encoder, pct: 25, color: '#8b5cf6' },
                { label: 'R3: Verifier', value: wallet.streams.verifier, pct: 15, color: '#10b981' },
                { label: 'R4: Storage', value: wallet.streams.storage, pct: 20, color: '#f59e0b' },
              ].map(s => (
                <div key={s.label} style={{ marginBottom: 'var(--ob-gap-md)' }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                    <span style={{ fontSize: '0.85rem' }}>{s.label} ({s.pct}%)</span>
                    <span style={{ fontWeight: 600, color: s.color }}>{formatObt(s.value)} OBT</span>
                  </div>
                  <div className="progress-bar" style={{ height: 6 }}>
                    <div className="fill" style={{
                      width: wallet.total_earned > 0 ? `${(s.value / wallet.total_earned) * 100}%` : '0%',
                      background: s.color,
                    }} />
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
                <div className="fill" style={{
                  width: wallet.rate_max > 0 ? `${(wallet.rate_used / wallet.rate_max) * 100}%` : '0%',
                }} />
              </div>
            </div>
          </div>

          {/* Staking Section */}
          <div className="glass-card animate-in" style={{ animationDelay: '450ms', marginBottom: 'var(--ob-gap-lg)' }}>
            <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)', display: 'flex', alignItems: 'center', gap: 8 }}>
              <Lock size={18} style={{ color: '#f59e0b' }} />{t('wallet.stake')}
            </h3>
            <p style={{ fontSize: '0.85rem', color: 'var(--ob-text-secondary)', marginBottom: 16 }}>
              {t('wallet.stakingInfo')}
            </p>
            <div style={{ display: 'flex', gap: 12, alignItems: 'center', marginBottom: 16 }}>
              <input
                type="number"
                value={stakeAmount}
                onChange={e => setStakeAmount(e.target.value)}
                placeholder={t('wallet.stakeAmount')}
                style={{
                  flex: 1, padding: '10px 14px', borderRadius: 8,
                  background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)',
                  color: 'var(--ob-text-primary)', fontSize: '0.9rem',
                }}
              />
              <button className="btn-primary" style={{ padding: '10px 24px', borderRadius: 8, display: 'flex', alignItems: 'center', gap: 6 }}>
                <Lock size={14} />{t('wallet.stake')}
              </button>
              <button style={{
                padding: '10px 24px', borderRadius: 8, display: 'flex', alignItems: 'center', gap: 6,
                background: 'transparent', border: '1px solid var(--ob-glass-border)', color: 'var(--ob-text-secondary)',
                cursor: 'pointer', fontSize: '0.88rem',
              }}>
                <Unlock size={14} />{t('wallet.unstake')}
              </button>
            </div>
            <div style={{
              display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 12,
            }}>
              {[
                { label: t('wallet.staked'), value: '0', color: '#f59e0b' },
                { label: t('wallet.earned'), value: formatObt(wallet.total_earned), color: '#10b981' },
                { label: t('wallet.pending'), value: '0', color: '#6366f1' },
              ].map(({ label, value, color }) => (
                <div key={label} style={{
                  padding: '12px 16px', borderRadius: 8,
                  background: 'var(--ob-bg-secondary)', textAlign: 'center',
                }}>
                  <div style={{ fontSize: '0.78rem', color: 'var(--ob-text-tertiary)', marginBottom: 4 }}>{label}</div>
                  <div style={{ fontSize: '1.1rem', fontWeight: 600, color }}>{value} OBT</div>
                </div>
              ))}
            </div>
          </div>

          {/* Transaction History */}
          <div className="glass-card animate-in" style={{ animationDelay: '500ms' }}>
            <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)', display: 'flex', alignItems: 'center', gap: 8 }}>
              <Clock size={18} style={{ color: 'var(--ob-violet)' }} /> Transaction History
            </h3>
            {txns.length === 0 ? (
              <div className="empty-state"><Coins size={40} /><p>No transactions yet</p></div>
            ) : (
              <table className="data-table">
                <thead><tr><th>Type</th><th>Amount</th><th>Detail</th><th>Status</th><th>Date</th></tr></thead>
                <tbody>
                  {txns.map((tx, i) => (
                    <tr key={i}>
                      <td>
                        <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                          {tx.amount >= 0 ? <ArrowDownRight size={14} style={{ color: 'var(--ob-success)' }} /> : <ArrowUpRight size={14} style={{ color: 'var(--ob-error)' }} />}
                          <span className={`badge ${tx.block_type === 'Mint' ? 'badge-green' : tx.block_type === 'Send' ? 'badge-amber' : 'badge-cyan'}`}>
                            {tx.block_type}
                          </span>
                        </div>
                      </td>
                      <td style={{ fontWeight: 600, color: tx.amount >= 0 ? 'var(--ob-success)' : 'var(--ob-error)' }}>
                        {tx.amount >= 0 ? '+' : ''}{formatObt(tx.amount)}
                      </td>
                      <td style={{ fontSize: '0.82rem', color: 'var(--ob-text-secondary)', maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {tx.detail}
                      </td>
                      <td><span className={`badge ${tx.confirmation === 'Settled' ? 'badge-green' : 'badge-amber'}`}>{tx.confirmation}</span></td>
                      <td style={{ fontSize: '0.78rem', color: 'var(--ob-text-tertiary)' }}>{formatDate(tx.timestamp)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </>
      )}
    </div>
  );
}
