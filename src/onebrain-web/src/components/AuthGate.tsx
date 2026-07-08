import { useState, type ReactNode } from 'react';
import { setToken } from '../api/client';
import { api } from '../api/client';

export function AuthGate({ children }: { children: ReactNode }) {
  const [hasToken, setHasToken] = useState(() => !!localStorage.getItem('ob_api_token'));
  const [input, setInput] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

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
