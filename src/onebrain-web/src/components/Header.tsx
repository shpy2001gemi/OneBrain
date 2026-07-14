import { useLocation, useNavigate } from 'react-router-dom';
import { Settings, Coins } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { StatusResponse } from '../api/types';

const pageTitles: Record<string, string> = {
  '/': 'Dashboard',
  '/explorer': 'Knowledge Explorer',
  '/encode': 'Encode Knowledge',
  '/chat': 'Chat',
  '/graph': 'Knowledge Graph',
  '/pomv': 'PoMV Monitor',
  '/network': 'Network',
  '/wallet': 'OBT Wallet',
  '/settings': 'Settings',
};

export function Header() {
  const location = useLocation();
  const navigate = useNavigate();
  const title = pageTitles[location.pathname] || 'OneBrain';
  const [status, setStatus] = useState<StatusResponse | null>(null);

  useEffect(() => {
    let abortCtrl: AbortController | null = null;
    let pending = false;

    const fetchStatus = async () => {
      if (pending) return; // skip if previous poll still in-flight
      pending = true;
      abortCtrl = new AbortController();
      const timer = setTimeout(() => abortCtrl?.abort(), 3000); // 3s timeout
      try {
        const res = await fetch('http://127.0.0.1:4280/api/status', {
          headers: {
            'Authorization': `Bearer ${localStorage.getItem('ob_api_token') || ''}`,
          },
          signal: abortCtrl.signal,
        });
        clearTimeout(timer);
        const json = await res.json();
        if (json.ok) setStatus(json.data);
      } catch {
        clearTimeout(timer);
        // timeout or network error — silently skip
      } finally {
        pending = false;
      }
    };

    fetchStatus();
    const interval = setInterval(fetchStatus, 10000);
    return () => { clearInterval(interval); abortCtrl?.abort(); };
  }, []);

  const formatObt = (milliObt: number) => {
    const obt = milliObt / 1000;
    return obt >= 1000 ? `${(obt / 1000).toFixed(1)}K` : obt.toFixed(1);
  };

  return (
    <header role="banner" aria-label="App header" style={{
      height: 'var(--ob-header-height)',
      borderBottom: '1px solid var(--ob-glass-border)',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: '0 24px',
      background: 'rgba(17, 24, 39, 0.6)',
      backdropFilter: 'blur(12px)',
    }}>
      <h2 style={{ fontSize: '1.1rem', fontWeight: 600 }}>{title}</h2>

      <div style={{ display: 'flex', alignItems: 'center', gap: '16px' }}>
        {status && (
          <>
            <span className="badge badge-cyan">{status.ku_count} KUs</span>
            <span className="badge badge-green">{status.peer_count} peers</span>
            <div style={{
              display: 'flex', alignItems: 'center', gap: '4px',
              color: 'var(--ob-warning)', fontSize: '0.85rem', fontWeight: 600,
            }}>
              <Coins size={16} />
              {formatObt(status.obt_balance)} OBT
            </div>
          </>
        )}
        <button className="btn btn-icon" style={{ color: 'var(--ob-text-secondary)' }} onClick={() => navigate('/settings')}>
          <Settings size={18} />
        </button>
      </div>
    </header>
  );
}
