import { useState, useEffect, useRef } from 'react';
import { Bug, X, Trash2, ChevronDown, ChevronRight } from 'lucide-react';
import { ws } from '../api/ws';

export interface DebugEntry {
  id: number;
  timestamp: Date;
  type: 'request' | 'response' | 'error' | 'ws' | 'info';
  method?: string;
  path?: string;
  status?: number;
  duration?: number;
  body?: string;
  message?: string;
}

let entryId = 0;
const debugListeners: Set<(entry: DebugEntry) => void> = new Set();

/** Call this from the API client to log requests */
export function logDebug(entry: Omit<DebugEntry, 'id' | 'timestamp'>) {
  const full: DebugEntry = { ...entry, id: ++entryId, timestamp: new Date() };
  debugListeners.forEach(fn => fn(full));
}

export function DebugConsole() {
  const [open, setOpen] = useState(false);
  const [entries, setEntries] = useState<DebugEntry[]>([]);
  const [expandedIds, setExpandedIds] = useState<Set<number>>(new Set());
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (entry: DebugEntry) => {
      setEntries(prev => [...prev.slice(-200), entry]); // keep last 200
    };
    debugListeners.add(handler);
    return () => { debugListeners.delete(handler); };
  }, []);

  // Listen to WebSocket events
  useEffect(() => {
    const unsub = ws.on('*', (event) => {
      logDebug({
        type: 'ws',
        message: `WS: ${event.event_type}`,
        body: JSON.stringify(event, null, 2),
      });
    });
    return unsub;
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries]);

  const toggleExpand = (id: number) => {
    setExpandedIds(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const typeColor = (type: DebugEntry['type']) => {
    switch (type) {
      case 'request': return '#4fc3f7';
      case 'response': return '#66bb6a';
      case 'error': return '#ef5350';
      case 'ws': return '#ab47bc';
      case 'info': return '#78909c';
    }
  };

  const typeLabel = (type: DebugEntry['type']) => {
    switch (type) {
      case 'request': return 'REQ';
      case 'response': return 'RES';
      case 'error': return 'ERR';
      case 'ws': return ' WS';
      case 'info': return 'INF';
    }
  };

  const formatTime = (d: Date) =>
    `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}.${d.getMilliseconds().toString().padStart(3, '0')}`;

  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        title="Debug Console"
        style={{
          position: 'fixed', bottom: 16, right: 16, zIndex: 9999,
          width: 40, height: 40, borderRadius: '50%',
          background: 'rgba(30, 30, 45, 0.9)', border: '1px solid rgba(100, 100, 140, 0.4)',
          color: '#78909c', cursor: 'pointer',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          backdropFilter: 'blur(10px)',
          transition: 'all 0.2s',
        }}
        onMouseEnter={e => { e.currentTarget.style.color = '#4fc3f7'; e.currentTarget.style.borderColor = '#4fc3f7'; }}
        onMouseLeave={e => { e.currentTarget.style.color = '#78909c'; e.currentTarget.style.borderColor = 'rgba(100, 100, 140, 0.4)'; }}
      >
        <Bug size={18} />
        {entries.some(e => e.type === 'error') && (
          <span style={{
            position: 'absolute', top: -2, right: -2,
            width: 10, height: 10, borderRadius: '50%',
            background: '#ef5350',
          }} />
        )}
      </button>
    );
  }

  return (
    <div style={{
      position: 'fixed', bottom: 0, right: 0, zIndex: 9999,
      width: 520, height: 380,
      background: 'rgba(12, 12, 20, 0.96)',
      borderTop: '1px solid rgba(100, 100, 140, 0.3)',
      borderLeft: '1px solid rgba(100, 100, 140, 0.3)',
      borderTopLeftRadius: 12,
      backdropFilter: 'blur(20px)',
      display: 'flex', flexDirection: 'column',
      fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
      fontSize: '0.72rem',
    }}>
      {/* Header */}
      <div style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        padding: '8px 12px',
        borderBottom: '1px solid rgba(100, 100, 140, 0.2)',
        background: 'rgba(20, 20, 35, 0.8)',
        borderTopLeftRadius: 12,
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: '#4fc3f7', fontWeight: 600 }}>
          <Bug size={14} /> Debug Console
          <span style={{ color: '#78909c', fontWeight: 400 }}>({entries.length})</span>
        </div>
        <div style={{ display: 'flex', gap: 4 }}>
          <button
            onClick={() => setEntries([])}
            style={{ background: 'none', border: 'none', color: '#78909c', cursor: 'pointer', padding: 4 }}
            title="Clear"
          >
            <Trash2 size={13} />
          </button>
          <button
            onClick={() => setOpen(false)}
            style={{ background: 'none', border: 'none', color: '#78909c', cursor: 'pointer', padding: 4 }}
            title="Close"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Entries */}
      <div ref={scrollRef} style={{ flex: 1, overflow: 'auto', padding: '4px 0' }}>
        {entries.length === 0 && (
          <div style={{ color: '#555', textAlign: 'center', padding: 40 }}>
            No activity yet. Interact with the dashboard to see debug logs.
          </div>
        )}
        {entries.map(entry => {
          const expanded = expandedIds.has(entry.id);
          return (
            <div key={entry.id} style={{ borderBottom: '1px solid rgba(60, 60, 80, 0.3)' }}>
              <div
                onClick={() => entry.body && toggleExpand(entry.id)}
                style={{
                  display: 'flex', alignItems: 'center', gap: 6,
                  padding: '3px 12px',
                  cursor: entry.body ? 'pointer' : 'default',
                  lineHeight: '1.6',
                }}
              >
                {entry.body ? (
                  expanded ? <ChevronDown size={10} color="#555" /> : <ChevronRight size={10} color="#555" />
                ) : <span style={{ width: 10 }} />}
                <span style={{ color: '#555', minWidth: 80 }}>{formatTime(entry.timestamp)}</span>
                <span style={{
                  color: typeColor(entry.type), fontWeight: 700,
                  minWidth: 28, textAlign: 'center',
                }}>
                  {typeLabel(entry.type)}
                </span>
                {entry.method && (
                  <span style={{ color: '#e0e0e0', fontWeight: 600, minWidth: 40 }}>{entry.method}</span>
                )}
                {entry.path && (
                  <span style={{ color: '#aaa' }}>{entry.path}</span>
                )}
                {entry.status !== undefined && (
                  <span style={{
                    color: entry.status < 400 ? '#66bb6a' : '#ef5350',
                    fontWeight: 600,
                  }}>
                    {entry.status}
                  </span>
                )}
                {entry.duration !== undefined && (
                  <span style={{ color: entry.duration > 5000 ? '#ff9800' : '#78909c' }}>
                    {entry.duration > 1000 ? `${(entry.duration / 1000).toFixed(1)}s` : `${entry.duration}ms`}
                  </span>
                )}
                {entry.message && (
                  <span style={{ color: typeColor(entry.type), opacity: 0.9 }}>{entry.message}</span>
                )}
              </div>
              {expanded && entry.body && (
                <pre style={{
                  margin: '0 12px 6px 40px',
                  padding: '6px 10px',
                  background: 'rgba(20, 20, 30, 0.8)',
                  borderRadius: 6,
                  color: '#b0bec5',
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-all',
                  maxHeight: 150,
                  overflow: 'auto',
                  fontSize: '0.68rem',
                  lineHeight: '1.5',
                }}>
                  {entry.body}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
