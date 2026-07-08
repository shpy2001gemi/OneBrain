import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Brain, Users, Coins, Clock, Zap, Search, MessageSquare, Cpu } from 'lucide-react';
import { api } from '../api/client';
import type { StatusResponse, KuListItem, AiHealthInfo } from '../api/types';

export function DashboardPage() {
  const navigate = useNavigate();
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [recentKus, setRecentKus] = useState<KuListItem[]>([]);
  const [aiHealth, setAiHealth] = useState<AiHealthInfo | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      api.getStatus().then(setStatus),
      api.listKus(1, 5).then(r => setRecentKus(r.kus)),
      api.aiStatus().then(setAiHealth).catch(() => {}),
    ]).finally(() => setLoading(false));
  }, []);

  const formatUptime = (s: number) => {
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  };

  const formatObt = (milli: number) => (milli / 1000).toFixed(1);

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
          { icon: Coins, label: 'OBT Balance', value: formatObt(status?.obt_balance ?? 0), sub: status?.tier ?? '', color: 'var(--ob-warning)' },
          { icon: Clock, label: 'Uptime', value: formatUptime(status?.uptime_s ?? 0), sub: `v${status?.version ?? '?'}`, color: 'var(--ob-success)' },
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
              <thead><tr><th>CID</th><th>Type</th><th>Preview</th><th>PoMV</th></tr></thead>
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
