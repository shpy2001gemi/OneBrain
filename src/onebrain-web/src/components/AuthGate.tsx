import { useEffect, useState, type ReactNode } from 'react';
import { setToken } from '../api/client';
import { api } from '../api/client';
import { getApiConfig, isTauri } from '../api/tauri';

function DesktopAuthGate({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [error, setError] = useState('Waiting for the local Desktop backend');
  useEffect(() => {
    let active = true;
    const cleanups: (() => void)[] = [];
    const read = async () => {
      try { await getApiConfig(); if (active) setReady(true); }
      catch {
        try {
          const { invoke } = await import('@tauri-apps/api/core');
          const status = await invoke<string>('desktop_lifecycle_status');
          if (active) setError(status);
        } catch {
          if (active) setError('Local backend is starting or unavailable. If it does not become ready, use the tray Restart action.');
        }
      }
    };
    void import('@tauri-apps/api/event').then(async ({ listen }) => {
      for (const event of ['backend-ready', 'desktop-lifecycle']) {
        const cleanup = await listen(event, () => { void read(); });
        if (active) cleanups.push(cleanup); else cleanup();
      }
      if (active) void read();
    }).catch(() => { if (active) setError('Desktop credential handoff unavailable'); });
    return () => { active = false; cleanups.forEach(cleanup => cleanup()); };
  }, []);
  return ready ? <>{children}</> : <main className="glass-card" role="status">{error}</main>;
}

export function AuthGate({ children }: { children: ReactNode }) {
  const [hasToken, setHasToken] = useState(() => !!localStorage.getItem('ob_api_token'));
  const [input, setInput] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  // Desktop mode: skip auth (managed by Tauri backend)
  if (isTauri()) {
    return <DesktopAuthGate>{children}</DesktopAuthGate>;
  }

  const handleSubmit = async () => {
    if (!input.trim()) return;
    setLoading(true);
    setError('');
    setToken(input.trim());
    try {
      await api.getStatus();
      setHasToken(true);
    } catch {
      setError('Cannot connect. Check token and ensure the node is running.');
      setToken('');
    } finally {
      setLoading(false);
    }
  };

  if (hasToken) return <>{children}</>;

  return (
    <div style={{
      height: '100vh',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      background: 'var(--ob-bg-primary)',
    }}>
      <div className="glass-card" style={{ width: 420, textAlign: 'center' }}>
        <div style={{ fontSize: '3rem', marginBottom: '16px' }}>🧠</div>
        <h1 style={{
          fontSize: '1.5rem',
          fontWeight: 700,
          marginBottom: '8px',
          background: 'linear-gradient(135deg, var(--ob-accent-light), var(--ob-violet))',
          WebkitBackgroundClip: 'text',
          WebkitTextFillColor: 'transparent',
        }}>OneBrain</h1>
        <p style={{ color: 'var(--ob-text-secondary)', marginBottom: '24px', fontSize: '0.9rem' }}>
          Enter your API token to connect to the local node.
        </p>
        <input
          className="input"
          type="password"
          placeholder="API Token"
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && handleSubmit()}
          style={{ marginBottom: '12px' }}
        />
        {error && (
          <p style={{ color: 'var(--ob-error)', fontSize: '0.8rem', marginBottom: '12px' }}>{error}</p>
        )}
        <button className="btn btn-primary btn-lg" onClick={handleSubmit} disabled={loading}
          style={{ width: '100%' }}>
          {loading ? <span className="spinner" /> : 'Connect'}
        </button>
      </div>
    </div>
  );
}
