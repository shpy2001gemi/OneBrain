import { useState, useEffect } from 'react';
import { api } from '../api/client';
import type { DeviceInfo, SyncStatus } from '../api/types';

const DEVICE_ICONS: Record<string, string> = {
  desktop: '💻',
  mobile: '📱',
  web: '🌐',
  cli: '⌨️',
};

const SYNC_BADGE: Record<string, { label: string; bg: string; color: string; dot: string }> = {
  'up-to-date': {
    label: 'Up to date',
    bg: 'rgba(16, 185, 129, 0.12)',
    color: 'var(--ob-success)',
    dot: 'var(--ob-success)',
  },
  syncing: {
    label: 'Syncing…',
    bg: 'rgba(245, 158, 11, 0.12)',
    color: '#f59e0b',
    dot: '#f59e0b',
  },
  offline: {
    label: 'Offline',
    bg: 'rgba(148, 163, 184, 0.10)',
    color: 'var(--ob-text-secondary)',
    dot: '#64748b',
  },
};

function formatTimestamp(ts: number): string {
  if (!ts) return '—';
  const d = new Date(ts * 1000);
  const now = Date.now();
  const diffMs = now - d.getTime();
  const diffMin = Math.floor(diffMs / 60_000);
  if (diffMin < 1) return 'Just now';
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffH = Math.floor(diffMin / 60);
  if (diffH < 24) return `${diffH}h ago`;
  const diffD = Math.floor(diffH / 24);
  if (diffD < 7) return `${diffD}d ago`;
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

export function DevicesPage() {
  const [syncStatus, setSyncStatus] = useState<SyncStatus | null>(null);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = async () => {
    try {
      const [sync, devs] = await Promise.all([
        api.syncStatus(),
        api.listDevices(),
      ]);
      setSyncStatus(sync);
      setDevices(devs);
      setError(null);
    } catch (e: any) {
      setError(e.message || 'Failed to load device data');
    }
    setLoading(false);
  };

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 15_000);
    return () => clearInterval(interval);
  }, []);

  const overallBadge = SYNC_BADGE[syncStatus?.status ?? 'offline'] ?? SYNC_BADGE.offline;

  // ─── Loading state ─────────────────────────
  if (loading) {
    return (
      <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}>
        <div className="spinner spinner-lg" />
      </div>
    );
  }

  return (
    <div className="page">
      {/* ─── Header ──────────────────────────── */}
      <div className="page-header">
        <h1>Devices</h1>
        <p>Multi-device management &amp; sync status</p>
      </div>

      {/* ─── Sync Overview Card ──────────────── */}
      <div
        className="glass-card animate-in"
        style={{
          marginBottom: 'var(--ob-gap-lg)',
          padding: 'var(--ob-gap-lg)',
          position: 'relative',
          overflow: 'hidden',
        }}
      >
        {/* Decorative gradient accent */}
        <div
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            right: 0,
            height: 3,
            background: 'linear-gradient(90deg, var(--ob-accent), var(--ob-accent-light), var(--ob-violet, #8b5cf6))',
            borderRadius: 'var(--ob-radius-md) var(--ob-radius-md) 0 0',
          }}
        />

        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 20 }}>
          <span style={{ fontSize: '1.5rem' }}>🔄</span>
          <h2 style={{ fontSize: '1.1rem', fontWeight: 600, color: 'var(--ob-text-primary)', margin: 0 }}>
            Sync Status
          </h2>
          {/* Overall badge */}
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 6,
              padding: '4px 12px',
              borderRadius: 20,
              fontSize: '0.78rem',
              fontWeight: 500,
              background: overallBadge.bg,
              color: overallBadge.color,
            }}
          >
            <span style={{
              width: 7,
              height: 7,
              borderRadius: '50%',
              background: overallBadge.dot,
              display: 'inline-block',
              boxShadow: `0 0 6px ${overallBadge.dot}`,
            }} />
            {overallBadge.label}
          </span>
        </div>

        {/* Stats row */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(160px, 1fr))', gap: 'var(--ob-gap-md)' }}>
          {[
            {
              label: 'Pending Items',
              value: syncStatus?.pending_count ?? 0,
              color: (syncStatus?.pending_count ?? 0) > 0 ? '#f59e0b' : 'var(--ob-success)',
            },
            {
              label: 'Last Sync',
              value: formatTimestamp(syncStatus?.last_sync ?? 0),
              color: 'var(--ob-accent-light)',
            },
            {
              label: 'Total Devices',
              value: devices.length,
              color: 'var(--ob-accent)',
            },
          ].map((stat, i) => (
            <div
              key={i}
              style={{
                background: 'var(--ob-bg-secondary)',
                borderRadius: 'var(--ob-radius-md)',
                padding: '16px 20px',
                border: '1px solid var(--ob-glass-border)',
              }}
            >
              <div style={{
                fontSize: '0.75rem',
                color: 'var(--ob-text-secondary)',
                textTransform: 'uppercase',
                letterSpacing: '0.05em',
                marginBottom: 6,
              }}>
                {stat.label}
              </div>
              <div style={{
                fontSize: '1.35rem',
                fontWeight: 700,
                color: stat.color,
              }}>
                {stat.value}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* ─── Error banner ────────────────────── */}
      {error && (
        <div
          className="animate-in"
          style={{
            marginBottom: 'var(--ob-gap-md)',
            padding: '12px 16px',
            borderRadius: 'var(--ob-radius-md)',
            background: 'rgba(239, 68, 68, 0.08)',
            border: '1px solid rgba(239, 68, 68, 0.25)',
            color: 'var(--ob-error)',
            fontSize: '0.85rem',
          }}
        >
          ⚠️ {error}
        </div>
      )}

      {/* ─── Device List ─────────────────────── */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 'var(--ob-gap-md)' }}>
        <h2 style={{ fontSize: '1.05rem', fontWeight: 600, color: 'var(--ob-text-primary)', margin: 0 }}>
          Registered Devices
        </h2>
        <button className="btn btn-sm" onClick={loadData}>
          🔄 Refresh
        </button>
      </div>

      {devices.length === 0 ? (
        <div className="glass-card animate-in" style={{ padding: 'var(--ob-gap-xl, 48px)', textAlign: 'center' }}>
          <div style={{ fontSize: '2.5rem', marginBottom: 12 }}>📡</div>
          <p style={{ color: 'var(--ob-text-secondary)', fontSize: '0.9rem', margin: 0 }}>
            No devices registered yet
          </p>
        </div>
      ) : (
        <div style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))',
          gap: 'var(--ob-gap-md)',
        }}>
          {devices.map((dev, i) => {
            const badge = SYNC_BADGE[dev.sync_status] ?? SYNC_BADGE.offline;
            const icon = DEVICE_ICONS[dev.device_type?.toLowerCase()] ?? '📟';

            return (
              <div
                key={dev.device_id}
                className="glass-card animate-in"
                style={{
                  animationDelay: `${i * 80}ms`,
                  padding: 'var(--ob-gap-lg)',
                  transition: 'var(--ob-transition)',
                  cursor: 'default',
                  position: 'relative',
                  overflow: 'hidden',
                }}
                onMouseEnter={e => {
                  (e.currentTarget as HTMLElement).style.borderColor = 'var(--ob-accent-light)';
                  (e.currentTarget as HTMLElement).style.transform = 'translateY(-2px)';
                }}
                onMouseLeave={e => {
                  (e.currentTarget as HTMLElement).style.borderColor = '';
                  (e.currentTarget as HTMLElement).style.transform = '';
                }}
              >
                {/* Top row: icon + name + badge */}
                <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', marginBottom: 16 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                    <span style={{
                      fontSize: '1.6rem',
                      width: 44,
                      height: 44,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      borderRadius: 'var(--ob-radius-md)',
                      background: 'var(--ob-bg-secondary)',
                      border: '1px solid var(--ob-glass-border)',
                      flexShrink: 0,
                    }}>
                      {icon}
                    </span>
                    <div>
                      <div style={{
                        fontSize: '0.95rem',
                        fontWeight: 600,
                        color: 'var(--ob-text-primary)',
                        lineHeight: 1.3,
                      }}>
                        {dev.name}
                      </div>
                      <div style={{
                        fontSize: '0.75rem',
                        color: 'var(--ob-text-secondary)',
                        textTransform: 'capitalize',
                        marginTop: 2,
                      }}>
                        {dev.device_type}
                      </div>
                    </div>
                  </div>

                  {/* Sync badge */}
                  <span style={{
                    display: 'inline-flex',
                    alignItems: 'center',
                    gap: 5,
                    padding: '3px 10px',
                    borderRadius: 16,
                    fontSize: '0.72rem',
                    fontWeight: 500,
                    background: badge.bg,
                    color: badge.color,
                    whiteSpace: 'nowrap',
                    flexShrink: 0,
                  }}>
                    <span style={{
                      width: 6,
                      height: 6,
                      borderRadius: '50%',
                      background: badge.dot,
                      display: 'inline-block',
                      boxShadow: `0 0 5px ${badge.dot}`,
                      animation: dev.sync_status === 'syncing' ? 'pulse 1.5s ease-in-out infinite' : 'none',
                    }} />
                    {badge.label}
                  </span>
                </div>

                {/* Divider */}
                <div style={{
                  height: 1,
                  background: 'var(--ob-glass-border)',
                  marginBottom: 14,
                }} />

                {/* Detail rows */}
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ fontSize: '0.8rem', color: 'var(--ob-text-secondary)' }}>
                      Last Seen
                    </span>
                    <span style={{ fontSize: '0.8rem', color: 'var(--ob-text-primary)', fontWeight: 500 }}>
                      {formatTimestamp(dev.last_seen)}
                    </span>
                  </div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ fontSize: '0.8rem', color: 'var(--ob-text-secondary)' }}>
                      KU Count
                    </span>
                    <span style={{
                      fontSize: '0.8rem',
                      color: 'var(--ob-accent-light)',
                      fontWeight: 600,
                      fontVariantNumeric: 'tabular-nums',
                    }}>
                      {dev.ku_count.toLocaleString()}
                    </span>
                  </div>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ fontSize: '0.8rem', color: 'var(--ob-text-secondary)' }}>
                      Device ID
                    </span>
                    <span
                      className="mono"
                      style={{
                        fontSize: '0.7rem',
                        color: 'var(--ob-text-secondary)',
                        maxWidth: 120,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                      }}
                      title={dev.device_id}
                    >
                      {dev.device_id.slice(0, 12)}…
                    </span>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Pulse animation for syncing dot */}
      <style>{`
        @keyframes pulse {
          0%, 100% { opacity: 1; transform: scale(1); }
          50% { opacity: 0.5; transform: scale(1.4); }
        }
      `}</style>
    </div>
  );
}
