import { useState } from 'react';
import { Database, Play } from 'lucide-react';
import { api } from '../api/client';

export function LocalKqlDiscovery() {
  const [query, setQuery] = useState('FIND (k:KU) SCOPE LOCAL LIMIT 20');
  const [results, setResults] = useState<unknown[] | null>(null);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  const run = async () => {
    if (!query.trim()) return;
    setLoading(true);
    setError('');
    try {
      const response = await api.kql(query.trim());
      setResults(response.results || []);
    } catch (reason) {
      setResults(null);
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setLoading(false);
    }
  };

  return (
    <section className="glass-card" style={{ marginBottom: 20 }}>
      <h2 style={{ fontSize: '1rem', display: 'flex', gap: 8, alignItems: 'center' }}>
        <Database size={18} /> Local KQL
      </h2>
      <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.82rem', margin: '6px 0 14px' }}>
        Executes only against this node&apos;s local store. It does not contact peers or create a StandingNeed.
      </p>
      <textarea
        className="input mono"
        rows={4}
        value={query}
        onChange={event => setQuery(event.target.value)}
        aria-label="Local KQL query"
        style={{ width: '100%', resize: 'vertical' }}
      />
      <button className="btn btn-primary" onClick={run} disabled={loading} style={{ marginTop: 10 }}>
        {loading ? <span className="spinner" /> : <><Play size={15} /> Run locally</>}
      </button>
      {error && (
        <div style={{ color: 'var(--ob-error)', fontSize: '0.82rem', marginTop: 12 }}>{error}</div>
      )}
      {results && (
        <div style={{ marginTop: 14 }}>
          <strong>{results.length} local result(s)</strong>
          {results.length === 0 && (
            <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.8rem' }}>
              No match in this local store. This says nothing about other nodes or the wider network.
            </p>
          )}
          {results.length > 0 && (
            <pre style={{ overflow: 'auto', maxHeight: 320, fontSize: '0.72rem' }}>
              {JSON.stringify(results, null, 2)}
            </pre>
          )}
        </div>
      )}
    </section>
  );
}
