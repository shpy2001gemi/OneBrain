import { useState } from 'react';
import { Activity, Search, Dna, Shield, TrendingUp } from 'lucide-react';
import { api } from '../api/client';
import type { KuDetail } from '../api/types';

const POMV_DIMS = [
  { key: 'metabolic', label: 'Metabolic', icon: Activity, color: '#06b6d4', desc: 'Usage frequency and recency' },
  { key: 'prediction', label: 'Prediction', icon: TrendingUp, color: '#8b5cf6', desc: 'Predictive accuracy of the KU' },
  { key: 'entropy', label: 'Entropy', icon: Dna, color: '#f59e0b', desc: 'Information richness' },
  { key: 'survival', label: 'Survival', icon: Shield, color: '#10b981', desc: 'Temporal resilience' },
  { key: 'centrality', label: 'Centrality', icon: Activity, color: '#3b82f6', desc: 'Graph connectivity importance' },
  { key: 'niche', label: 'Niche', icon: Activity, color: '#ec4899', desc: 'Domain specialization value' },
] as const;

export function PomvPage() {
  const [cidInput, setCidInput] = useState('');
  const [ku, setKu] = useState<KuDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const loadKu = async () => {
    if (!cidInput.trim()) return;
    setLoading(true);
    setError('');
    try {
      const detail = await api.getKu(cidInput.trim());
      setKu(detail);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'KU not found');
    } finally {
      setLoading(false);
    }
  };

  const pomvScore = ku ? ku.pomv * 100 : 0;
  const breakdown = ku?.pomv_breakdown;

  return (
    <div className="page">
      <div className="page-header">
        <h1>PoMV Monitor</h1>
        <p>Proof of Metabolic Value — knowledge lifecycle analysis</p>
      </div>

      {/* CID Input */}
      <div style={{ display: 'flex', gap: 'var(--ob-gap-md)', marginBottom: 'var(--ob-gap-lg)', alignItems: 'center' }}>
        <div style={{ flex: 1, position: 'relative' }}>
          <Activity size={16} style={{ position: 'absolute', left: 12, top: '50%', transform: 'translateY(-50%)', color: 'var(--ob-text-muted)' }} />
          <input className="input" placeholder="Enter KU CID to analyze..." value={cidInput}
            onChange={e => setCidInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && loadKu()}
            style={{ paddingLeft: 36 }} />
        </div>
        <button className="btn btn-primary" onClick={loadKu} disabled={loading}>
          {loading ? <span className="spinner" /> : <><Search size={16} /> Analyze</>}
        </button>
      </div>

      {error && <div className="glass-card" style={{ borderColor: 'rgba(239,68,68,0.3)', color: 'var(--ob-error)', marginBottom: 'var(--ob-gap-md)' }}>{error}</div>}

      {!ku && !error && (
        <div className="glass-card empty-state" style={{ minHeight: 400 }}>
          <Activity size={48} />
          <h3 style={{ color: 'var(--ob-text-secondary)' }}>Select a Knowledge Unit</h3>
          <p style={{ fontSize: '0.85rem' }}>Enter a CID to view its metabolic lifecycle and PoMV breakdown</p>
        </div>
      )}

      {ku && breakdown && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--ob-gap-lg)' }}>
          {/* Top row: Overall score + KU info */}
          <div className="grid-3" style={{ gridTemplateColumns: '1fr 2fr' }}>
            {/* Score */}
            <div className="glass-card accent-glow animate-in" style={{ textAlign: 'center' }}>
              <span className="stat-label">Overall PoMV</span>
              <div style={{
                fontSize: '3.5rem', fontWeight: 800, margin: '16px 0',
                background: `linear-gradient(135deg, ${pomvScore > 60 ? '#10b981' : pomvScore > 30 ? '#f59e0b' : '#ef4444'}, var(--ob-accent-light))`,
                WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent',
              }}>{pomvScore.toFixed(1)}%</div>
              <div style={{ display: 'flex', gap: 'var(--ob-gap-sm)', justifyContent: 'center', flexWrap: 'wrap' }}>
                <span className="badge badge-cyan">{ku.gene_type}</span>
                <span className="badge badge-violet">{ku.epistemic}</span>
                <span className="badge badge-green">{ku.verification_status}</span>
              </div>
            </div>

            {/* KU Info */}
            <div className="glass-card animate-in" style={{ animationDelay: '100ms' }}>
              <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)' }}>Knowledge Unit</h3>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--ob-gap-sm)' }}>
                <div><span className="stat-label">CID</span><p className="mono" style={{ fontSize: '0.75rem', wordBreak: 'break-all' }}>{ku.cid_hex}</p></div>
                <div><span className="stat-label">Content</span><p style={{ fontSize: '0.85rem', lineHeight: 1.5, maxHeight: 80, overflow: 'hidden' }}>{ku.content}</p></div>
                <div className="grid-4" style={{ marginTop: 'var(--ob-gap-sm)' }}>
                  <div><span className="stat-label">Trust</span><p style={{ fontWeight: 600 }}>{(ku.trust * 100).toFixed(0)}%</p></div>
                  <div><span className="stat-label">Confidence</span><p style={{ fontWeight: 600 }}>{(ku.confidence * 100).toFixed(0)}%</p></div>
                  <div><span className="stat-label">Wire Size</span><p style={{ fontWeight: 600 }}>{ku.wire_size}B</p></div>
                  <div><span className="stat-label">Bonds</span><p style={{ fontWeight: 600 }}>{ku.outgoing_bond_count + ku.incoming_bond_count}</p></div>
                </div>
              </div>
            </div>
          </div>

          {/* PoMV Dimensions */}
          <div className="glass-card animate-in" style={{ animationDelay: '200ms' }}>
            <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-lg)' }}>PoMV Dimensions</h3>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--ob-gap-md)' }}>
              {POMV_DIMS.map(({ key, label, color, desc }) => {
                const val = breakdown[key] as number;
                const pct = val * 100;
                return (
                  <div key={key}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        <div style={{ width: 8, height: 8, borderRadius: '50%', background: color }} />
                        <span style={{ fontWeight: 500, fontSize: '0.88rem' }}>{label}</span>
                        <span style={{ color: 'var(--ob-text-muted)', fontSize: '0.75rem' }}>{desc}</span>
                      </div>
                      <span style={{ fontWeight: 600, fontSize: '0.88rem', color }}>{pct.toFixed(1)}%</span>
                    </div>
                    <div className="progress-bar" style={{ height: 6 }}>
                      <div className="fill" style={{ width: `${pct}%`, background: color }} />
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
