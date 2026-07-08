import { useEffect, useState } from 'react';
import { Wifi, WifiOff, Plus, RefreshCw, Globe } from 'lucide-react';
import { api } from '../api/client';
import type { PeerView, StatusResponse } from '../api/types';

export function NetworkPage() {
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [peers, setPeers] = useState<PeerView[]>([]);
  const [connectAddr, setConnectAddr] = useState('');
  const [connecting, setConnecting] = useState(false);
  const [connectResult, setConnectResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const loadData = async () => {
    try {
      const [s, p] = await Promise.all([api.getStatus(), api.getPeers()]);
      setStatus(s);
      setPeers(p.peers);
    } catch { /* ignore */ }
    setLoading(false);
  };

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 10000);
    return () => clearInterval(interval);
  }, []);

  const handleConnect = async () => {
    if (!connectAddr.trim() || connecting) return;
    setConnecting(true);
    setConnectResult(null);
    try {
      await api.connectPeer(connectAddr.trim());
      setConnectResult('Connected successfully!');
      setConnectAddr('');
      loadData();
    } catch (e: any) {
      setConnectResult(`Error: ${e.message}`);
    } finally {
      setConnecting(false);
    }
  };

  const formatUptime = (s: number) => {
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  };

  if (loading) {
    return <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}>
      <div className="spinner spinner-lg" />
    </div>;
  }

  return (
    <div className="page">
      <div className="page-header">
        <h1>Network</h1>
        <p>P2P network status and peer management</p>
      </div>

      {/* Stats */}
      <div className="grid-4" style={{ marginBottom: 'var(--ob-gap-lg)' }}>
        {[
          { icon: Globe, label: 'Node Name', value: status?.node_name ?? '—', color: 'var(--ob-accent)' },
          { icon: Wifi, label: 'Connected Peers', value: status?.peer_count ?? 0, color: 'var(--ob-success)' },
          { icon: RefreshCw, label: 'Uptime', value: formatUptime(status?.uptime_s ?? 0), color: 'var(--ob-violet)' },
          { icon: Globe, label: 'Version', value: `v${status?.version ?? '?'}`, color: 'var(--ob-text-secondary)' },
        ].map((s, i) => (
          <div key={i} className="glass-card stat-card animate-in" style={{ animationDelay: `${i * 80}ms` }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
              <span className="stat-label">{s.label}</span>
              <s.icon size={18} style={{ color: s.color, opacity: 0.7 }} />
            </div>
            <span className="stat-value">{s.value}</span>
          </div>
        ))}
      </div>

      <div className="grid-3" style={{ gridTemplateColumns: '2fr 1fr' }}>
        {/* Peer List */}
        <div className="glass-card animate-in" style={{ animationDelay: '300ms' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 'var(--ob-gap-md)' }}>
            <h3 style={{ fontSize: '1rem', fontWeight: 600 }}>Connected Peers</h3>
            <button className="btn btn-sm" onClick={loadData}><RefreshCw size={14} /> Refresh</button>
          </div>
          {peers.length === 0 ? (
            <div className="empty-state">
              <WifiOff size={40} />
              <p>No peers connected</p>
              <p style={{ fontSize: '0.8rem' }}>Use the connect form to add a peer</p>
            </div>
          ) : (
            <table className="data-table">
              <thead><tr><th>Name</th><th>Address</th><th>KUs</th><th>Status</th></tr></thead>
              <tbody>
                {peers.map((p, i) => (
                  <tr key={i}>
                    <td style={{ fontWeight: 500 }}>{p.name}</td>
                    <td className="mono" style={{ fontSize: '0.78rem' }}>{p.addr}</td>
                    <td>{p.ku_count}</td>
                    <td><span className="badge badge-green">Connected</span></td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        {/* Connect Form */}
        <div className="glass-card animate-in" style={{ animationDelay: '400ms' }}>
          <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)', display: 'flex', alignItems: 'center', gap: 8 }}>
            <Plus size={18} style={{ color: 'var(--ob-accent)' }} /> Connect Peer
          </h3>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--ob-gap-sm)' }}>
            <input className="input" placeholder="IP:Port (e.g. 192.168.1.10:4200)"
              value={connectAddr} onChange={e => setConnectAddr(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleConnect()} />
            <button className="btn btn-primary" onClick={handleConnect} disabled={connecting}>
              {connecting ? <span className="spinner" /> : <><Wifi size={16} /> Connect</>}
            </button>
            {connectResult && (
              <p style={{ fontSize: '0.82rem', color: connectResult.startsWith('Error') ? 'var(--ob-error)' : 'var(--ob-success)' }}>
                {connectResult}
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
