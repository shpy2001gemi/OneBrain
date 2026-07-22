import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import type { FollowedNode, PeerProfile, WatchInfo, KuListItem } from '../api/types';
import { GENE_TYPE_COLORS } from '../api/types';
import { formatDateShort } from '../utils/format';

// ─── Inline Styles ──────────────────────────────────────────
const styles = {
  page: {
    padding: 'var(--ob-gap-lg)',
    maxWidth: 960,
    margin: '0 auto',
  } as React.CSSProperties,

  header: {
    marginBottom: 'var(--ob-gap-lg)',
  } as React.CSSProperties,

  title: {
    fontSize: '1.6rem',
    fontWeight: 700,
    color: 'var(--ob-text-primary)',
    margin: 0,
  } as React.CSSProperties,

  subtitle: {
    fontSize: '0.88rem',
    color: 'var(--ob-text-secondary)',
    marginTop: 4,
  } as React.CSSProperties,

  tabBar: {
    display: 'flex',
    gap: 4,
    background: 'var(--ob-bg-secondary)',
    borderRadius: 'var(--ob-radius-md)',
    padding: 4,
    marginBottom: 'var(--ob-gap-lg)',
    border: '1px solid var(--ob-glass-border)',
  } as React.CSSProperties,

  tab: (active: boolean): React.CSSProperties => ({
    flex: 1,
    padding: '10px 16px',
    border: 'none',
    borderRadius: 'calc(var(--ob-radius-md) - 2px)',
    cursor: 'pointer',
    fontSize: '0.88rem',
    fontWeight: active ? 600 : 400,
    color: active ? 'var(--ob-text-primary)' : 'var(--ob-text-secondary)',
    background: active ? 'var(--ob-accent-light)' : 'transparent',
    transition: 'var(--ob-transition)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
  }),

  card: {
    background: 'var(--ob-bg-secondary)',
    border: '1px solid var(--ob-glass-border)',
    borderRadius: 'var(--ob-radius-md)',
    padding: 'var(--ob-gap-md)',
    marginBottom: 'var(--ob-gap-md)',
    transition: 'var(--ob-transition)',
  } as React.CSSProperties,

  inputRow: {
    display: 'flex',
    gap: 'var(--ob-gap-sm)',
    marginBottom: 'var(--ob-gap-md)',
  } as React.CSSProperties,

  input: {
    flex: 1,
    padding: '10px 14px',
    background: 'var(--ob-bg-secondary)',
    border: '1px solid var(--ob-glass-border)',
    borderRadius: 'var(--ob-radius-md)',
    color: 'var(--ob-text-primary)',
    fontSize: '0.88rem',
    outline: 'none',
    transition: 'var(--ob-transition)',
  } as React.CSSProperties,

  btnPrimary: {
    padding: '10px 20px',
    background: 'var(--ob-accent)',
    color: '#fff',
    border: 'none',
    borderRadius: 'var(--ob-radius-md)',
    cursor: 'pointer',
    fontSize: '0.85rem',
    fontWeight: 600,
    transition: 'var(--ob-transition)',
    whiteSpace: 'nowrap' as const,
  } as React.CSSProperties,

  btnDanger: {
    padding: '6px 14px',
    background: 'transparent',
    color: 'var(--ob-error)',
    border: '1px solid var(--ob-error)',
    borderRadius: 'var(--ob-radius-md)',
    cursor: 'pointer',
    fontSize: '0.78rem',
    fontWeight: 500,
    transition: 'var(--ob-transition)',
  } as React.CSSProperties,

  listItem: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: '12px 16px',
    background: 'var(--ob-bg-secondary)',
    border: '1px solid var(--ob-glass-border)',
    borderRadius: 'var(--ob-radius-md)',
    marginBottom: 8,
    transition: 'var(--ob-transition)',
  } as React.CSSProperties,

  listItemInfo: {
    display: 'flex',
    flexDirection: 'column' as const,
    gap: 2,
  } as React.CSSProperties,

  listItemName: {
    fontSize: '0.92rem',
    fontWeight: 600,
    color: 'var(--ob-text-primary)',
  } as React.CSSProperties,

  listItemMeta: {
    fontSize: '0.78rem',
    color: 'var(--ob-text-secondary)',
    fontFamily: 'monospace',
  } as React.CSSProperties,

  profileCard: {
    background: 'var(--ob-bg-secondary)',
    border: '1px solid var(--ob-glass-border)',
    borderRadius: 'var(--ob-radius-md)',
    padding: 'var(--ob-gap-lg)',
    marginTop: 'var(--ob-gap-md)',
  } as React.CSSProperties,

  profileName: {
    fontSize: '1.3rem',
    fontWeight: 700,
    color: 'var(--ob-text-primary)',
    marginBottom: 4,
  } as React.CSSProperties,

  profileId: {
    fontSize: '0.78rem',
    color: 'var(--ob-text-secondary)',
    fontFamily: 'monospace',
    marginBottom: 'var(--ob-gap-md)',
    wordBreak: 'break-all' as const,
  } as React.CSSProperties,

  statGrid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fit, minmax(130px, 1fr))',
    gap: 'var(--ob-gap-sm)',
    marginBottom: 'var(--ob-gap-md)',
  } as React.CSSProperties,

  statBox: {
    background: 'rgba(255,255,255,0.03)',
    border: '1px solid var(--ob-glass-border)',
    borderRadius: 'var(--ob-radius-md)',
    padding: '12px 14px',
    textAlign: 'center' as const,
  } as React.CSSProperties,

  statValue: {
    fontSize: '1.2rem',
    fontWeight: 700,
    color: 'var(--ob-text-primary)',
    display: 'block',
  } as React.CSSProperties,

  statLabel: {
    fontSize: '0.72rem',
    color: 'var(--ob-text-secondary)',
    textTransform: 'uppercase' as const,
    letterSpacing: '0.5px',
    marginTop: 4,
    display: 'block',
  } as React.CSSProperties,

  badge: (color: string): React.CSSProperties => ({
    display: 'inline-block',
    padding: '3px 10px',
    borderRadius: 20,
    fontSize: '0.72rem',
    fontWeight: 600,
    background: `${color}22`,
    color,
    border: `1px solid ${color}44`,
  }),

  expertiseList: {
    display: 'flex',
    flexWrap: 'wrap' as const,
    gap: 6,
    marginTop: 'var(--ob-gap-sm)',
  } as React.CSSProperties,

  expertiseTag: {
    padding: '4px 10px',
    borderRadius: 12,
    fontSize: '0.76rem',
    background: 'var(--ob-accent-light)',
    color: 'var(--ob-text-primary)',
    border: '1px solid var(--ob-glass-border)',
  } as React.CSSProperties,

  empty: {
    textAlign: 'center' as const,
    padding: '40px 20px',
    color: 'var(--ob-text-secondary)',
    fontSize: '0.88rem',
  } as React.CSSProperties,

  emptyIcon: {
    fontSize: '2.4rem',
    marginBottom: 12,
    display: 'block',
    opacity: 0.5,
  } as React.CSSProperties,

  error: {
    padding: '10px 14px',
    background: 'rgba(239,68,68,0.08)',
    border: '1px solid rgba(239,68,68,0.25)',
    borderRadius: 'var(--ob-radius-md)',
    color: 'var(--ob-error)',
    fontSize: '0.84rem',
    marginBottom: 'var(--ob-gap-md)',
  } as React.CSSProperties,

  success: {
    padding: '10px 14px',
    background: 'rgba(16,185,129,0.08)',
    border: '1px solid rgba(16,185,129,0.25)',
    borderRadius: 'var(--ob-radius-md)',
    color: 'var(--ob-success)',
    fontSize: '0.84rem',
    marginBottom: 'var(--ob-gap-md)',
  } as React.CSSProperties,

  sectionTitle: {
    fontSize: '0.92rem',
    fontWeight: 600,
    color: 'var(--ob-text-primary)',
    marginBottom: 'var(--ob-gap-sm)',
    display: 'flex',
    alignItems: 'center',
    gap: 8,
  } as React.CSSProperties,

  spinnerWrap: {
    display: 'flex',
    justifyContent: 'center',
    padding: '40px 0',
  } as React.CSSProperties,
} as const;

// ─── Helpers ────────────────────────────────────────────────
function formatDate(ts: number): string {
  return formatDateShort(ts);
}

function truncateId(id: string, len = 12): string {
  if (id.length <= len * 2 + 3) return id;
  return `${id.slice(0, len)}…${id.slice(-len)}`;
}

// ─── Tab Components ─────────────────────────────────────────

function FollowingTab() {
  const [nodes, setNodes] = useState<FollowedNode[]>([]);
  const [loading, setLoading] = useState(true);
  const [newNodeId, setNewNodeId] = useState('');
  const [following, setFollowing] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await api.listFollowing();
      setNodes(data);
    } catch (e: unknown) {
      setMessage({ type: 'error', text: e instanceof Error ? e.message : String(e) });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleFollow = async () => {
    const id = newNodeId.trim();
    if (!id || following) return;
    setFollowing(true);
    setMessage(null);
    try {
      await api.followNode(id);
      setNewNodeId('');
      setMessage({ type: 'success', text: `Now following node ${truncateId(id)}` });
      await load();
    } catch (e: unknown) {
      setMessage({ type: 'error', text: e instanceof Error ? e.message : String(e) });
    } finally {
      setFollowing(false);
    }
  };

  const handleUnfollow = async (nodeId: string) => {
    setMessage(null);
    try {
      await api.unfollowNode(nodeId);
      setMessage({ type: 'success', text: `Unfollowed ${truncateId(nodeId)}` });
      await load();
    } catch (e: unknown) {
      setMessage({ type: 'error', text: e instanceof Error ? e.message : String(e) });
    }
  };

  return (
    <div>
      <div style={styles.sectionTitle}>
        <span>➕ Follow a New Node</span>
      </div>
      <div style={styles.inputRow}>
        <input
          style={styles.input}
          placeholder="Enter node ID to follow…"
          value={newNodeId}
          onChange={e => setNewNodeId(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && handleFollow()}
        />
        <button style={styles.btnPrimary} onClick={handleFollow} disabled={following}>
          {following ? '…' : 'Follow'}
        </button>
      </div>

      {message && (
        <div style={message.type === 'error' ? styles.error : styles.success}>
          {message.text}
        </div>
      )}

      <div style={styles.sectionTitle}>
        <span>👥 Following ({nodes.length})</span>
      </div>

      {loading ? (
        <div style={styles.spinnerWrap}><div className="spinner spinner-lg" /></div>
      ) : nodes.length === 0 ? (
        <div style={styles.empty}>
          <span style={styles.emptyIcon}>👥</span>
          <p>You're not following anyone yet</p>
          <p style={{ fontSize: '0.8rem', marginTop: 4 }}>Enter a node ID above to start following</p>
        </div>
      ) : (
        nodes.map(node => (
          <div key={node.node_id} style={styles.listItem}>
            <div style={styles.listItemInfo}>
              <span style={styles.listItemName}>{node.name || 'Unknown Node'}</span>
              <span style={styles.listItemMeta}>{truncateId(node.node_id)}</span>
              <span style={{ fontSize: '0.74rem', color: 'var(--ob-text-secondary)' }}>
                Since {formatDate(node.followed_at)}
              </span>
            </div>
            <button style={styles.btnDanger} onClick={() => handleUnfollow(node.node_id)}>
              Unfollow
            </button>
          </div>
        ))
      )}
    </div>
  );
}

function ProfilesTab() {
  const [nodeId, setNodeId] = useState('');
  const [profile, setProfile] = useState<PeerProfile | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleLookup = async () => {
    const id = nodeId.trim();
    if (!id || loading) return;
    setLoading(true);
    setError(null);
    setProfile(null);
    try {
      const data = await api.getPeerProfile(id);
      setProfile(data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  const tierColor = (tier: string): string => {
    switch (tier.toLowerCase()) {
      case 'gold': return '#f59e0b';
      case 'silver': return '#94a3b8';
      case 'platinum': return '#818cf8';
      default: return '#64748b';
    }
  };

  const trustColor = (score: number): string => {
    if (score >= 0.8) return 'var(--ob-success)';
    if (score >= 0.5) return '#f59e0b';
    return 'var(--ob-error)';
  };

  return (
    <div>
      <div style={styles.sectionTitle}>
        <span>🔍 Look Up Peer Profile</span>
      </div>
      <div style={styles.inputRow}>
        <input
          style={styles.input}
          placeholder="Enter node ID…"
          value={nodeId}
          onChange={e => setNodeId(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && handleLookup()}
        />
        <button style={styles.btnPrimary} onClick={handleLookup} disabled={loading}>
          {loading ? '…' : 'Lookup'}
        </button>
      </div>

      {error && <div style={styles.error}>{error}</div>}

      {loading && (
        <div style={styles.spinnerWrap}><div className="spinner spinner-lg" /></div>
      )}

      {profile && (
        <div style={styles.profileCard}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', flexWrap: 'wrap', gap: 12 }}>
            <div>
              <div style={styles.profileName}>{profile.name || 'Anonymous'}</div>
              <div style={styles.profileId}>{profile.node_id}</div>
            </div>
            <span style={styles.badge(tierColor(profile.tier))}>
              {profile.tier.toUpperCase()}
            </span>
          </div>

          <div style={styles.statGrid}>
            <div style={styles.statBox}>
              <span style={{ ...styles.statValue, color: trustColor(profile.trust_score) }}>
                {(profile.trust_score * 100).toFixed(1)}%
              </span>
              <span style={styles.statLabel}>Trust Score</span>
            </div>
            <div style={styles.statBox}>
              <span style={styles.statValue}>{profile.ku_count.toLocaleString()}</span>
              <span style={styles.statLabel}>KU Count</span>
            </div>
            <div style={styles.statBox}>
              <span style={styles.statValue}>{formatDate(profile.member_since)}</span>
              <span style={styles.statLabel}>Member Since</span>
            </div>
          </div>

          {profile.expertise.length > 0 && (
            <div>
              <div style={{ fontSize: '0.8rem', color: 'var(--ob-text-secondary)', marginBottom: 6 }}>
                Expertise
              </div>
              <div style={styles.expertiseList}>
                {profile.expertise.map((e, i) => (
                  <span key={i} style={styles.expertiseTag}>{e}</span>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {!loading && !profile && !error && (
        <div style={styles.empty}>
          <span style={styles.emptyIcon}>🔍</span>
          <p>Enter a node ID to view their profile</p>
        </div>
      )}
    </div>
  );
}

function WatchesTab() {
  const [watches, setWatches] = useState<WatchInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [newQuery, setNewQuery] = useState('');
  const [creating, setCreating] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await api.listWatches();
      setWatches(data);
    } catch (e: unknown) {
      setMessage({ type: 'error', text: e instanceof Error ? e.message : String(e) });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleCreate = async () => {
    const q = newQuery.trim();
    if (!q || creating) return;
    setCreating(true);
    setMessage(null);
    try {
      const result = await api.createWatch(q);
      setNewQuery('');
      setMessage({ type: 'success', text: `Watch created: ${result.watch_id.slice(0, 8)}…` });
      await load();
    } catch (e: unknown) {
      setMessage({ type: 'error', text: e instanceof Error ? e.message : String(e) });
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (watchId: string) => {
    if (!confirm('Unsubscribe from this watch?')) return;
    setMessage(null);
    try {
      await api.deleteWatch(watchId);
      setMessage({ type: 'success', text: `Watch deleted` });
      await load();
    } catch (e: unknown) {
      setMessage({ type: 'error', text: e instanceof Error ? e.message : String(e) });
    }
  };

  return (
    <div>
      <div style={styles.sectionTitle}>
        <span>✨ Create Watch Query</span>
      </div>
      <div style={styles.inputRow}>
        <input
          style={styles.input}
          placeholder="Enter a standing query…"
          value={newQuery}
          onChange={e => setNewQuery(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && handleCreate()}
        />
        <button style={styles.btnPrimary} onClick={handleCreate} disabled={creating}>
          {creating ? '…' : 'Create'}
        </button>
      </div>

      {message && (
        <div style={message.type === 'error' ? styles.error : styles.success}>
          {message.text}
        </div>
      )}

      <div style={styles.sectionTitle}>
        <span>👁️ Active Watches ({watches.length})</span>
      </div>

      {loading ? (
        <div style={styles.spinnerWrap}><div className="spinner spinner-lg" /></div>
      ) : watches.length === 0 ? (
        <div style={styles.empty}>
          <span style={styles.emptyIcon}>👁️</span>
          <p>No active watch queries</p>
          <p style={{ fontSize: '0.8rem', marginTop: 4 }}>Create one to get notified about matching KUs</p>
        </div>
      ) : (
        watches.map(w => (
          <div key={w.watch_id} style={styles.listItem}>
            <div style={styles.listItemInfo}>
              <span style={styles.listItemName}>{w.query}</span>
              <div style={{ display: 'flex', gap: 12, marginTop: 4 }}>
                <span style={styles.listItemMeta}>
                  ID: {truncateId(w.watch_id, 6)}
                </span>
                <span style={{ fontSize: '0.76rem', color: 'var(--ob-text-secondary)' }}>
                  Created {formatDate(w.created)}
                </span>
                <span style={{ fontSize: '0.76rem', color: 'var(--ob-accent)' }}>
                  {w.match_count} match{w.match_count !== 1 ? 'es' : ''}
                </span>
              </div>
            </div>
            <button style={styles.btnDanger} onClick={() => handleDelete(w.watch_id)}>
              Delete
            </button>
          </div>
        ))
      )}
    </div>
  );
}

// ─── Feed Tab (Trending Feed) ────────────────────────────────

type TrendingItem = { ku: KuListItem; trend_score: number; reason: string };

function FeedTab() {
  const [items, setItems] = useState<TrendingItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.getTrending(20)
      .then(d => setItems(d.trending || []))
      .catch(() => setItems([]))
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <div style={{ textAlign: 'center', padding: 40 }}><div className="spinner" /></div>;

  if (items.length === 0) {
    return (
      <div style={{ textAlign: 'center', padding: '40px 20px', color: 'var(--ob-text-tertiary)' }}>
        <div style={{ fontSize: '2rem', marginBottom: 8 }}>🔥</div>
        <p>No trending KUs yet. Start encoding knowledge!</p>
      </div>
    );
  }

  return (
    <div style={{ display: 'grid', gap: 12 }}>
      {items.map((item, i) => {
        const color = GENE_TYPE_COLORS[item.ku.gene_type] || '#6366f1';
        return (
          <div key={item.ku.cid_hex} style={{
            padding: '14px 18px', borderRadius: 10,
            background: 'var(--ob-bg-secondary)',
            borderLeft: `3px solid ${color}`,
            display: 'flex', alignItems: 'center', gap: 14,
          }}>
            <div style={{
              width: 28, height: 28, borderRadius: '50%',
              background: i < 3 ? 'linear-gradient(135deg, #f59e0b, #ef4444)' : 'var(--ob-bg-tertiary)',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontWeight: 700, fontSize: '0.8rem', flexShrink: 0,
              color: i < 3 ? '#fff' : 'var(--ob-text-secondary)',
            }}>{i + 1}</div>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 2 }}>
                <span style={{
                  padding: '1px 6px', borderRadius: 4, fontSize: '0.7rem', fontWeight: 600,
                  background: `${color}20`, color,
                }}>{item.ku.gene_type}</span>
                <span style={{ fontSize: '0.72rem', color: 'var(--ob-text-tertiary)', fontFamily: 'var(--ob-font-mono)' }}>
                  {item.ku.cid_hex.slice(0, 10)}…
                </span>
              </div>
              <div style={{
                fontSize: '0.88rem', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                color: 'var(--ob-text-primary)',
              }}>{item.ku.preview}</div>
            </div>
            <div style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)', textAlign: 'right', flexShrink: 0 }}>
              <div>Score: {(item.trend_score * 100).toFixed(0)}%</div>
              <div style={{ fontStyle: 'italic' }}>{item.reason.replace(/_/g, ' ')}</div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ─── Reputation Tab (MOCK DATA) ─────────────────────────────
// ⚠️ MOCK DATA NOTICE: The reputation system currently uses hardcoded
// mock data. This is intentional — real reputation scoring requires
// integration with the P2P verification consensus protocol (PoMV)
// and cross-node validation. Replace with real API calls when:
// 1. PoMV consensus finalization is implemented (backend)
// 2. Reputation aggregation endpoint is added to handlers.rs
// 3. Cross-node reputation sync is operational
// See: DEFERRED.md → Reputation System

const MOCK_REPUTATION = {
  score: 87,
  level: 'Expert',
  badges: [
    { name: 'Early Adopter', icon: '🌱', earned: '2024-01-15' },
    { name: 'Knowledge Sharer', icon: '📚', earned: '2024-03-22' },
    { name: 'Verified Contributor', icon: '✅', earned: '2024-06-01' },
    { name: 'Network Builder', icon: '🌐', earned: '2024-08-10' },
  ],
  stats: {
    kus_published: 142,
    kus_verified: 98,
    peers_helped: 23,
    avg_pomv: 0.73,
    trust_score: 0.89,
    uptime_days: 187,
  },
  history: [
    { date: '2024-12', score: 87 },
    { date: '2024-11', score: 82 },
    { date: '2024-10', score: 78 },
    { date: '2024-09', score: 71 },
    { date: '2024-08', score: 65 },
    { date: '2024-07', score: 58 },
  ],
};

function ReputationTab() {
  const rep = MOCK_REPUTATION;
  const scoreColor = rep.score >= 80 ? 'var(--ob-success)' : rep.score >= 50 ? 'var(--ob-warning)' : 'var(--ob-error)';

  return (
    <div>
      {/* Mock Data Notice */}
      <div style={{
        padding: '10px 14px', borderRadius: 8, marginBottom: 16,
        background: 'rgba(234, 179, 8, 0.1)', border: '1px solid rgba(234, 179, 8, 0.3)',
        fontSize: '0.78rem', color: 'rgb(234, 179, 8)',
      }}>
        ⚠️ <strong>Mock Data:</strong> Reputation scores are simulated. Real scoring requires PoMV consensus integration.
      </div>

      {/* Score Overview */}
      <div style={{ display: 'flex', gap: 20, marginBottom: 20, flexWrap: 'wrap' }}>
        <div style={{
          flex: '0 0 140px', textAlign: 'center', padding: 20,
          borderRadius: 12, background: 'var(--ob-surface)',
          border: '1px solid var(--ob-glass-border)',
        }}>
          <div style={{ fontSize: '2.8rem', fontWeight: 800, color: scoreColor, lineHeight: 1 }}>
            {rep.score}
          </div>
          <div style={{ fontSize: '0.78rem', color: 'var(--ob-text-muted)', marginTop: 4 }}>
            Reputation Score
          </div>
          <div style={{
            display: 'inline-block', padding: '3px 10px', borderRadius: 20, marginTop: 8,
            background: `${scoreColor}22`, color: scoreColor, fontSize: '0.75rem', fontWeight: 600,
          }}>
            {rep.level}
          </div>
        </div>

        <div style={{ flex: 1, minWidth: 200 }}>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(120px, 1fr))', gap: 8 }}>
            {[
              { label: 'KUs Published', value: rep.stats.kus_published },
              { label: 'KUs Verified', value: rep.stats.kus_verified },
              { label: 'Peers Helped', value: rep.stats.peers_helped },
              { label: 'Avg PoMV', value: rep.stats.avg_pomv.toFixed(2) },
              { label: 'Trust Score', value: `${(rep.stats.trust_score * 100).toFixed(0)}%` },
              { label: 'Uptime', value: `${rep.stats.uptime_days}d` },
            ].map(s => (
              <div key={s.label} style={{
                padding: '10px 12px', borderRadius: 8,
                background: 'var(--ob-surface)', border: '1px solid var(--ob-glass-border)',
              }}>
                <div style={{ fontSize: '1.1rem', fontWeight: 700, color: 'var(--ob-text-primary)' }}>{s.value}</div>
                <div style={{ fontSize: '0.7rem', color: 'var(--ob-text-muted)' }}>{s.label}</div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Badges */}
      <div style={{ marginBottom: 20 }}>
        <h3 style={{ fontSize: '0.9rem', fontWeight: 600, marginBottom: 10 }}>🏅 Badges</h3>
        <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
          {rep.badges.map(b => (
            <div key={b.name} style={{
              padding: '10px 14px', borderRadius: 10,
              background: 'var(--ob-surface)', border: '1px solid var(--ob-glass-border)',
              display: 'flex', alignItems: 'center', gap: 8,
            }}>
              <span style={{ fontSize: '1.4rem' }}>{b.icon}</span>
              <div>
                <div style={{ fontWeight: 600, fontSize: '0.82rem' }}>{b.name}</div>
                <div style={{ fontSize: '0.68rem', color: 'var(--ob-text-muted)' }}>Earned: {b.earned}</div>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Score History */}
      <div>
        <h3 style={{ fontSize: '0.9rem', fontWeight: 600, marginBottom: 10 }}>📈 Score History</h3>
        <div style={{
          display: 'flex', gap: 4, alignItems: 'flex-end', height: 100,
          padding: '0 8px',
        }}>
          {rep.history.slice().reverse().map(h => {
            const height = Math.max(10, h.score);
            return (
              <div key={h.date} style={{ flex: 1, textAlign: 'center' }}>
                <div style={{
                  height, borderRadius: '4px 4px 0 0',
                  background: `linear-gradient(180deg, var(--ob-accent), var(--ob-violet))`,
                  opacity: 0.8,
                  transition: 'height 0.3s',
                }} />
                <div style={{ fontSize: '0.6rem', color: 'var(--ob-text-muted)', marginTop: 4 }}>{h.date}</div>
                <div style={{ fontSize: '0.65rem', fontWeight: 600, color: 'var(--ob-text-secondary)' }}>{h.score}</div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ─── Main Page ──────────────────────────────────────────────

type TabKey = 'feed' | 'following' | 'profiles' | 'watches' | 'reputation';

const TABS: { key: TabKey; label: string; icon: string }[] = [
  { key: 'feed',      label: 'Feed',      icon: '🔥' },
  { key: 'following', label: 'Following', icon: '👥' },
  { key: 'profiles',  label: 'Profiles',  icon: '🔍' },
  { key: 'watches',   label: 'Watches',   icon: '👁️' },
  { key: 'reputation', label: 'Reputation', icon: '⭐' },
];

export function SocialPage() {
  const [tab, setTab] = useState<TabKey>('feed');

  return (
    <div className="page" style={styles.page}>
      <div style={styles.header}>
        <h1 style={styles.title}>Social & Discovery</h1>
        <p style={styles.subtitle}>Follow nodes, explore peer profiles, and manage watch queries</p>
      </div>

      <div style={styles.tabBar}>
        {TABS.map(t => (
          <button
            key={t.key}
            style={styles.tab(tab === t.key)}
            onClick={() => setTab(t.key)}
          >
            <span>{t.icon}</span>
            {t.label}
          </button>
        ))}
      </div>

      <div style={styles.card}>
        {tab === 'feed'      && <FeedTab />}
        {tab === 'following' && <FollowingTab />}
        {tab === 'profiles'  && <ProfilesTab />}
        {tab === 'watches'   && <WatchesTab />}
        {tab === 'reputation' && <ReputationTab />}
      </div>
    </div>
  );
}
