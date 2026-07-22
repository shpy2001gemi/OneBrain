import { useState } from 'react';
import { GitBranch, ChevronDown, ChevronUp, Clock } from 'lucide-react';

interface VersionEntry {
  cid_hex: string;
  gene_type: string;
  preview: string;
  version: number;
  created: number;
}

interface Props {
  versions: VersionEntry[];
  currentCid: string;
  onNavigate: (cid: string) => void;
}

function formatDate(ts: number): string {
  const d = new Date(ts * 1000);
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
    + ' ' + d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

export function VersionTimeline({ versions, currentCid, onNavigate }: Props) {
  const [expanded, setExpanded] = useState(false);

  if (!versions || versions.length <= 1) return null;

  const sorted = [...versions].sort((a, b) => b.version - a.version);

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        style={{
          background: 'none', border: 'none', cursor: 'pointer', padding: 0, width: '100%',
          display: 'flex', alignItems: 'center', gap: 6, color: 'var(--ob-text-secondary)',
        }}
        aria-expanded={expanded}
        aria-label={`Version history: ${versions.length} versions`}
      >
        <GitBranch size={14} />
        <span className="stat-label" style={{ margin: 0, flex: 1, textAlign: 'left' }}>
          Version History ({versions.length})
        </span>
        {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
      </button>

      {expanded && (
        <div style={{
          marginTop: 8, display: 'flex', flexDirection: 'column', gap: 0,
          borderLeft: '2px solid rgba(99, 102, 241, 0.4)', marginLeft: 7, paddingLeft: 16,
        }}>
          {sorted.map((v) => {
            const isCurrent = v.cid_hex === currentCid;
            return (
              <div
                key={v.cid_hex}
                onClick={() => !isCurrent && onNavigate(v.cid_hex)}
                style={{
                  position: 'relative', padding: '10px 12px', marginBottom: 4,
                  borderRadius: 'var(--ob-radius-sm)',
                  background: isCurrent ? 'rgba(99, 102, 241, 0.15)' : 'var(--ob-surface)',
                  border: isCurrent ? '1px solid rgba(99, 102, 241, 0.4)' : '1px solid transparent',
                  cursor: isCurrent ? 'default' : 'pointer',
                  transition: 'all 0.2s',
                  opacity: isCurrent ? 1 : 0.8,
                }}
                onMouseEnter={e => { if (!isCurrent) (e.currentTarget.style.opacity = '1'); }}
                onMouseLeave={e => { if (!isCurrent) (e.currentTarget.style.opacity = '0.8'); }}
                role="button"
                aria-label={`Version ${v.version}${isCurrent ? ' (current)' : ''}`}
                tabIndex={0}
                onKeyDown={e => { if (e.key === 'Enter' && !isCurrent) onNavigate(v.cid_hex); }}
              >
                {/* Timeline dot */}
                <div style={{
                  position: 'absolute', left: -22, top: 14, width: 10, height: 10,
                  borderRadius: '50%',
                  background: isCurrent ? '#6366f1' : 'rgba(99, 102, 241, 0.4)',
                  border: '2px solid var(--ob-bg-secondary)',
                }} />

                <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                  <span style={{
                    fontSize: '0.72rem', fontWeight: 700,
                    background: isCurrent ? '#6366f1' : 'rgba(99, 102, 241, 0.3)',
                    color: isCurrent ? '#fff' : '#c7d2fe',
                    padding: '1px 8px', borderRadius: 10,
                  }}>
                    v{v.version}
                  </span>
                  <span className="badge badge-cyan" style={{ fontSize: '0.68rem', padding: '1px 6px' }}>
                    {v.gene_type}
                  </span>
                  {isCurrent && (
                    <span style={{
                      fontSize: '0.68rem', color: '#22c55e', fontWeight: 600, marginLeft: 'auto',
                    }}>
                      ● Current
                    </span>
                  )}
                </div>

                <p style={{
                  fontSize: '0.78rem', color: 'var(--ob-text-secondary)',
                  margin: '2px 0', lineHeight: 1.4,
                  overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                }}>
                  {v.preview || '(empty)'}
                </p>

                <div style={{
                  display: 'flex', alignItems: 'center', gap: 4,
                  fontSize: '0.7rem', color: 'var(--ob-text-muted)', marginTop: 4,
                }}>
                  <Clock size={10} />
                  {formatDate(v.created)}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
