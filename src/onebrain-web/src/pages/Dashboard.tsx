import { useEffect, useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { Brain, Users, Coins, Clock, Zap, Search, MessageSquare, Cpu } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { PieChart, Pie, Cell, Tooltip, ResponsiveContainer, BarChart, Bar, XAxis, YAxis, CartesianGrid } from 'recharts';
import { api } from '../api/client';
import type { StatusResponse, KuListItem, AiHealthInfo } from '../api/types';
import { GENE_TYPE_COLORS, type GeneType } from '../api/types';
import { formatObt, formatDuration } from '../utils/format';

export function DashboardPage() {
  const navigate = useNavigate();
  useTranslation(); // TODO: replace hardcoded English strings with t() calls
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [recentKus, setRecentKus] = useState<KuListItem[]>([]);
  const [allKus, setAllKus] = useState<KuListItem[]>([]);
  const [aiHealth, setAiHealth] = useState<AiHealthInfo | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      api.getStatus().then(setStatus),
      api.listKus(1, 5).then(r => setRecentKus(r.kus)),
      api.listKus(1, 100).then(r => setAllKus(r.kus)).catch(() => {}),
      api.aiStatus().then(setAiHealth).catch(() => {}),
    ]).finally(() => setLoading(false));
  }, []);

  // Compute gene type distribution for pie chart
  const geneDistribution = useMemo(() => {
    const counts: Record<string, number> = {};
    allKus.forEach(ku => {
      counts[ku.gene_type] = (counts[ku.gene_type] || 0) + 1;
    });
    return Object.entries(counts)
      .map(([name, value]) => ({ name, value, color: GENE_TYPE_COLORS[name as GeneType] || '#64748b' }))
      .sort((a, b) => b.value - a.value);
  }, [allKus]);

  // Top PoMV scores for bar chart
  const pomvData = useMemo(() => {
    return [...allKus]
      .sort((a, b) => b.pomv - a.pomv)
      .slice(0, 8)
      .map(ku => ({
        name: ku.cid_hex.slice(0, 6),
        pomv: Math.round(ku.pomv * 100),
        type: ku.gene_type,
      }));
  }, [allKus]);


  if (loading) {
    return <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}>
      <div className="spinner spinner-lg" />
    </div>;
  }

  return (
    <div className="page">
      <div className="page-header">
        <h1>Dashboard</h1>
        <p>Overview of your OneBrain node</p>
      </div>

      {/* Stats */}
      <div className="grid-4" style={{ marginBottom: 'var(--ob-gap-lg)' }}>
        {[
          { icon: Brain, label: 'Knowledge Units', value: status?.ku_count ?? 0, sub: 'encoded', color: 'var(--ob-accent)' },
          { icon: Users, label: 'Connected Peers', value: status?.peer_count ?? 0, sub: 'active', color: 'var(--ob-violet)' },
          { icon: Coins, label: 'OBT Simulation', value: formatObt(status?.obt_balance ?? 0), sub: 'non-economic placeholder', color: 'var(--ob-warning)' },
          { icon: Clock, label: 'Uptime', value: formatDuration(status?.uptime_s ?? 0), sub: `v${status?.version ?? '?'}`, color: 'var(--ob-success)' },
        ].map((s, i) => (
          <div key={i} className="glass-card stat-card animate-in" style={{ animationDelay: `${i * 80}ms` }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
              <span className="stat-label">{s.label}</span>
              <s.icon size={18} style={{ color: s.color, opacity: 0.7 }} />
            </div>
            <span className="stat-value">{s.value}</span>
            <span className="stat-sub">{s.sub}</span>
          </div>
        ))}
      </div>

      {/* Quick Computed Stats */}
      {allKus.length > 0 && (
        <div className="grid-4" style={{ marginBottom: 'var(--ob-gap-lg)' }}>
          {(() => {
            const avgPomv = allKus.reduce((s, k) => s + k.pomv, 0) / allKus.length;
            const totalSize = allKus.reduce((s, k) => s + k.wire_size, 0);
            const avgTrust = allKus.reduce((s, k) => s + k.trust, 0) / allKus.length;
            const uniqueGenes = new Set(allKus.map(k => k.gene_type)).size;
            return [
              { label: 'Avg legacy PoMV', value: `${(avgPomv * 100).toFixed(0)}%`, color: avgPomv >= 0.6 ? 'var(--ob-success)' : 'var(--ob-warning)' },
              { label: 'Total Data', value: totalSize >= 1048576 ? `${(totalSize / 1048576).toFixed(1)} MB` : `${(totalSize / 1024).toFixed(0)} KB`, color: 'var(--ob-accent)' },
              { label: 'Avg Trust', value: `${(avgTrust * 100).toFixed(0)}%`, color: avgTrust >= 0.7 ? 'var(--ob-success)' : 'var(--ob-warning)' },
              { label: 'Gene Types', value: uniqueGenes, color: 'var(--ob-violet)' },
            ].map((s, i) => (
              <div key={i} className="glass-card animate-in" style={{
                animationDelay: `${(i + 4) * 80}ms`,
                padding: '12px 16px', textAlign: 'center',
              }}>
                <div style={{ fontSize: '1.4rem', fontWeight: 800, color: s.color }}>{s.value}</div>
                <div style={{ fontSize: '0.72rem', color: 'var(--ob-text-muted)', marginTop: 2 }}>{s.label}</div>
              </div>
            ));
          })()}
        </div>
      )}

      {/* Charts Row */}
      {allKus.length > 0 && (
        <div className="grid-2" style={{ marginBottom: 'var(--ob-gap-lg)' }}>
          {/* Gene Type Distribution */}
          <div className="glass-card animate-in" style={{ animationDelay: '250ms' }}>
            <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)' }}>Gene Type Distribution</h3>
            <div style={{ width: '100%', height: 220 }}>
              <ResponsiveContainer>
                <PieChart>
                  <Pie
                    data={geneDistribution}
                    cx="50%" cy="50%"
                    innerRadius={50} outerRadius={85}
                    paddingAngle={3}
                    dataKey="value"
                    animationDuration={800}
                  >
                    {geneDistribution.map((entry, i) => (
                      <Cell key={i} fill={entry.color} stroke="none" />
                    ))}
                  </Pie>
                  <Tooltip
                    contentStyle={{
                      background: 'rgba(17, 24, 39, 0.95)',
                      border: '1px solid rgba(255,255,255,0.1)',
                      borderRadius: 8, fontSize: '0.82rem',
                      color: '#e5e7eb',
                    }}
                  />
                </PieChart>
              </ResponsiveContainer>
            </div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 8 }}>
              {geneDistribution.slice(0, 6).map(d => (
                <span key={d.name} style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: '0.72rem', color: 'var(--ob-text-secondary)' }}>
                  <span style={{ width: 8, height: 8, borderRadius: '50%', background: d.color }} />
                  {d.name}: {d.value}
                </span>
              ))}
            </div>
          </div>

          {/* Top legacy local PoMV scalar scores */}
          <div className="glass-card animate-in" style={{ animationDelay: '350ms' }}>
            <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)' }}>Top legacy local PoMV scalars (non-economic)</h3>
            <div style={{ width: '100%', height: 250 }}>
              <ResponsiveContainer>
                <BarChart data={pomvData} margin={{ top: 5, right: 10, left: -10, bottom: 5 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.05)" />
                  <XAxis dataKey="name" tick={{ fill: '#9ca3af', fontSize: 11 }} />
                  <YAxis tick={{ fill: '#9ca3af', fontSize: 11 }} />
                  <Tooltip
                    contentStyle={{
                      background: 'rgba(17, 24, 39, 0.95)',
                      border: '1px solid rgba(255,255,255,0.1)',
                      borderRadius: 8, fontSize: '0.82rem',
                      color: '#e5e7eb',
                    }}
                    formatter={(value) => [`${value}%`, 'Legacy PoMV']}
                  />
                  <Bar dataKey="pomv" fill="url(#pomvGradient)" radius={[4, 4, 0, 0]} animationDuration={800} />
                  <defs>
                    <linearGradient id="pomvGradient" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="#06b6d4" />
                      <stop offset="100%" stopColor="#8b5cf6" />
                    </linearGradient>
                  </defs>
                </BarChart>
              </ResponsiveContainer>
            </div>
          </div>
        </div>
      )}

      {/* Content Row */}
      <div className="grid-2" style={{ gridTemplateColumns: '2fr 1fr', marginBottom: 'var(--ob-gap-lg)' }}>
        {/* Recent KUs */}
        <div className="glass-card animate-in" style={{ animationDelay: '300ms' }}>
          <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)' }}>Recent Knowledge</h3>
          {recentKus.length === 0 ? (
            <div className="empty-state">
              <Brain size={40} />
              <p>No knowledge encoded yet</p>
              <button className="btn btn-primary" onClick={() => navigate('/encode')}>Encode First KU</button>
            </div>
          ) : (
            <table className="data-table">
              <thead><tr><th>CID</th><th>Type</th><th>Preview</th><th>Legacy PoMV</th></tr></thead>
              <tbody>
                {recentKus.map(ku => (
                  <tr key={ku.cid_hex} style={{ cursor: 'pointer' }} onClick={() => navigate(`/explorer?cid=${ku.cid_hex}`)}>
                    <td className="mono">{ku.cid_hex.slice(0, 8)}…</td>
                    <td><span className="badge badge-cyan">{ku.gene_type}</span></td>
                    <td style={{ maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{ku.preview}</td>
                    <td>{(ku.pomv * 100).toFixed(0)}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        {/* AI Status */}
        <div className="glass-card animate-in" style={{ animationDelay: '400ms' }}>
          <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)', display: 'flex', alignItems: 'center', gap: 8 }}>
            <Cpu size={18} style={{ color: 'var(--ob-violet)' }} /> AI Engine
          </h3>
          {aiHealth ? (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--ob-text-secondary)', fontSize: '0.85rem' }}>Status</span>
                <span className={`badge ${aiHealth.connected ? 'badge-green' : 'badge-amber'}`}>
                  {aiHealth.connected ? 'Connected' : 'Offline'}
                </span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--ob-text-secondary)', fontSize: '0.85rem' }}>Model</span>
                <span style={{ fontFamily: 'var(--ob-font-mono)', fontSize: '0.82rem' }}>{aiHealth.model}</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--ob-text-secondary)', fontSize: '0.85rem' }}>Latency</span>
                <span style={{ fontSize: '0.85rem' }}>{aiHealth.latency_ms}ms</span>
              </div>
            </div>
          ) : (
            <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.85rem' }}>Unable to reach AI engine</p>
          )}
        </div>
      </div>

      {/* Quick Actions */}
      <div className="glass-card animate-in" style={{ animationDelay: '500ms' }}>
        <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)' }}>Quick Actions</h3>
        <div style={{ display: 'flex', gap: 'var(--ob-gap-md)' }}>
          <button className="btn btn-primary" onClick={() => navigate('/encode')}><Zap size={16} /> Encode Knowledge</button>
          <button className="btn" onClick={() => navigate('/explorer')}><Search size={16} /> Search KUs</button>
          <button className="btn" onClick={() => navigate('/chat')}><MessageSquare size={16} /> Open Chat</button>
        </div>
      </div>
    </div>
  );
}
