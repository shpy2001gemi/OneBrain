import { useState, useEffect, useRef } from 'react';
import { Zap, CheckCircle, AlertCircle, FileText, Copy, Loader, Clock } from 'lucide-react';
import { api } from '../api/client';
import type { EncodeResult } from '../api/types';

export function EncodePage() {
  const [text, setText] = useState('');
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<EncodeResult | null>(null);
  const [error, setError] = useState('');
  const [history, setHistory] = useState<EncodeResult[]>([]);
  const [elapsed, setElapsed] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Elapsed timer during encoding
  useEffect(() => {
    if (loading) {
      setElapsed(0);
      timerRef.current = setInterval(() => setElapsed(prev => prev + 1), 1000);
    } else {
      if (timerRef.current) { clearInterval(timerRef.current); timerRef.current = null; }
    }
    return () => { if (timerRef.current) clearInterval(timerRef.current); };
  }, [loading]);

  const handleEncode = async () => {
    if (!text.trim() || loading) return;
    setLoading(true);
    setError('');
    setResult(null);
    try {
      const res = await api.encode(text);
      setResult(res);
      setHistory(prev => [res, ...prev]);
      setText('');
    } catch (err: any) {
      setError(err.message || 'Encoding failed');
    } finally {
      setLoading(false);
    }
  };

  const copyToClipboard = (s: string) => {
    navigator.clipboard.writeText(s).catch(() => {});
  };

  const formatElapsed = (s: number) => {
    const min = Math.floor(s / 60);
    const sec = s % 60;
    return min > 0 ? `${min}m ${sec}s` : `${sec}s`;
  };

  // Real-time progress from WebSocket
  const [pipelineStep, setPipelineStep] = useState<{ step: number; total: number; message: string } | null>(null);
  const [stepLog, setStepLog] = useState<{ step: number; message: string; time: number }[]>([]);
  const wsRef = useRef<WebSocket | null>(null);

  // WebSocket connection for encode progress
  useEffect(() => {
    const token = localStorage.getItem('ob_api_token') || 'onebrain-dev-token';
    const ws = new WebSocket(`ws://127.0.0.1:4280/ws/events?token=${token}`);
    wsRef.current = ws;

    ws.onmessage = (e) => {
      try {
        const event = JSON.parse(e.data);
        if (event.event_type === 'encode_progress') {
          const { step, total_steps, message } = event.data;
          setPipelineStep({ step, total: total_steps, message });
          setStepLog(prev => {
            // avoid duplicate steps
            if (prev.length > 0 && prev[prev.length - 1].step === step) return prev;
            return [...prev, { step, message, time: Date.now() }];
          });
        }
      } catch {}
    };

    ws.onerror = () => {};
    ws.onclose = () => {};

    return () => { ws.close(); };
  }, []);

  // Reset step log when starting new encode
  const handleEncodeWithReset = async () => {
    setPipelineStep(null);
    setStepLog([]);
    handleEncode();
  };

  return (
    <div className="page">
      <div className="page-header">
        <h1>Encode Knowledge</h1>
        <p>Transform text into Knowledge Units with biological encoding</p>
      </div>

      <div className="grid-3" style={{ gridTemplateColumns: '1fr 1fr', gap: 'var(--ob-gap-lg)' }}>
        {/* Input */}
        <div className="glass-card animate-in">
          <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 'var(--ob-gap-md)', display: 'flex', alignItems: 'center', gap: 8 }}>
            <FileText size={18} style={{ color: 'var(--ob-accent)' }} /> Input Text
          </h3>
          <textarea
            className="input"
            placeholder="Paste or type knowledge to encode...&#10;&#10;Example: Photosynthesis is the process by which plants convert light energy into chemical energy stored in glucose."
            value={text}
            onChange={e => setText(e.target.value)}
            style={{ minHeight: 240, marginBottom: 'var(--ob-gap-md)' }}
          />
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span style={{ fontSize: '0.78rem', color: 'var(--ob-text-tertiary)' }}>
              {text.length} characters
            </span>
            <button
              className="btn btn-primary btn-lg"
              onClick={handleEncodeWithReset}
              disabled={loading || !text.trim()}
            >
              {loading ? <span className="spinner" /> : <><Zap size={16} /> Encode</>}
            </button>
          </div>
        </div>

        {/* Result */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--ob-gap-md)' }}>
          {/* Progress indicator while encoding */}
          {loading && (() => {
            const pct = pipelineStep ? (pipelineStep.step / pipelineStep.total) * 100 : 5;
            return (
              <div className="glass-card animate-in" style={{ borderColor: 'rgba(79, 195, 247, 0.3)' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
                  <Loader size={18} style={{ color: 'var(--ob-accent)', animation: 'spin 1s linear infinite' }} />
                  <span style={{ fontWeight: 600, color: 'var(--ob-accent)' }}>
                    Encoding — Step {pipelineStep?.step || '...'}/{pipelineStep?.total || '...'}
                  </span>
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 10 }}>
                  <Clock size={14} style={{ color: 'var(--ob-text-secondary)' }} />
                  <span style={{ fontSize: '0.85rem', color: 'var(--ob-text-secondary)' }}>
                    Elapsed: <strong style={{ color: elapsed > 60 ? 'var(--ob-warning)' : 'var(--ob-text-primary)' }}>{formatElapsed(elapsed)}</strong>
                  </span>
                </div>
                <div style={{
                  background: 'rgba(255,255,255,0.06)', borderRadius: 6, height: 6,
                  marginBottom: 10, overflow: 'hidden',
                }}>
                  <div style={{
                    width: `${pct}%`, height: '100%',
                    background: 'linear-gradient(90deg, var(--ob-accent), var(--ob-accent-hover))',
                    borderRadius: 6, transition: 'width 0.5s ease',
                  }} />
                </div>
                {/* Step log */}
                <div style={{ fontSize: '0.75rem', fontFamily: 'monospace', maxHeight: 140, overflowY: 'auto' }}>
                  {stepLog.map((s, i) => (
                    <div key={i} style={{ display: 'flex', gap: 8, padding: '2px 0', color: i === stepLog.length - 1 ? 'var(--ob-accent)' : 'var(--ob-text-tertiary)' }}>
                      <span style={{ color: 'var(--ob-success)', minWidth: 14 }}>✓</span>
                      <span>[{s.step}/{pipelineStep?.total}]</span>
                      <span>{s.message}</span>
                    </div>
                  ))}
                  {pipelineStep && stepLog.length > 0 && stepLog[stepLog.length - 1].step === pipelineStep.step && (
                    <div style={{ display: 'flex', gap: 8, padding: '2px 0', color: 'var(--ob-accent)', animation: 'pulse 1.5s ease-in-out infinite' }}>
                      <span style={{ minWidth: 14 }}>⏳</span>
                      <span>Working...</span>
                    </div>
                  )}
                </div>
              </div>
            );
          })()}

          {error && (
            <div className="glass-card animate-in" style={{ borderColor: 'rgba(239, 68, 68, 0.3)' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: 'var(--ob-error)' }}>
                <AlertCircle size={18} /> {error}
              </div>
            </div>
          )}

          {result && (
            <div className="glass-card accent-glow animate-in">
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 'var(--ob-gap-md)', color: 'var(--ob-success)' }}>
                <CheckCircle size={20} /> <span style={{ fontWeight: 600 }}>Encoded Successfully!</span>
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                <div>
                  <span className="stat-label">CID</span>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <code className="mono" style={{ fontSize: '0.78rem', wordBreak: 'break-all' }}>{result.cid_hex}</code>
                    <button className="btn btn-icon" onClick={() => copyToClipboard(result.cid_hex)} title="Copy CID">
                      <Copy size={14} />
                    </button>
                  </div>
                </div>
                <div className="grid-2">
                  <div className="stat-card">
                    <span className="stat-label">Gene Type</span>
                    <span className="badge badge-cyan" style={{ alignSelf: 'flex-start' }}>{result.gene_type || 'Unknown'}</span>
                  </div>
                  <div className="stat-card">
                    <span className="stat-label">Confidence</span>
                    <span className="stat-value" style={{ fontSize: '1.2rem' }}>{(result.confidence * 100).toFixed(1)}%</span>
                  </div>
                  <div className="stat-card">
                    <span className="stat-label">Wire Size</span>
                    <span style={{ fontSize: '0.95rem', fontWeight: 600 }}>{result.wire_size} bytes</span>
                  </div>
                  <div className="stat-card">
                    <span className="stat-label">Instructions</span>
                    <span style={{ fontSize: '0.95rem', fontWeight: 600 }}>{result.instruction_count}</span>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* History */}
          {history.length > 0 && (
            <div className="glass-card animate-in">
              <h3 style={{ fontSize: '0.9rem', fontWeight: 600, marginBottom: 'var(--ob-gap-sm)' }}>Session History ({history.length})</h3>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                {history.map((h, i) => (
                  <div key={i} style={{
                    display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                    padding: '8px 12px', borderRadius: 'var(--ob-radius-sm)', background: 'var(--ob-surface)',
                    fontSize: '0.82rem',
                  }}>
                    <span className="mono">{h.cid_hex.slice(0, 12)}…</span>
                    <span className="badge badge-cyan">{h.gene_type || '?'}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
