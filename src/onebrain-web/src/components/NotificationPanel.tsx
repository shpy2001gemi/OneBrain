import { useRef, useEffect } from 'react';
import { Bell, X, CheckCheck, Trash2, Info, CheckCircle, AlertTriangle, AlertCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { Notification } from '../hooks/useNotifications';

interface NotificationPanelProps {
  notifications: Notification[];
  unreadCount: number;
  isOpen: boolean;
  onToggle: () => void;
  onMarkRead: (id: string) => void;
  onMarkAllRead: () => void;
  onDismiss: (id: string) => void;
  onClearAll: () => void;
}

const ICON_MAP = {
  info: Info,
  success: CheckCircle,
  warning: AlertTriangle,
  error: AlertCircle,
};

const COLOR_MAP = {
  info: '#60a5fa',
  success: '#34d399',
  warning: '#fbbf24',
  error: '#f87171',
};

function timeAgo(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000);
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

export function NotificationPanel({
  notifications, unreadCount, isOpen, onToggle, onMarkRead, onMarkAllRead, onDismiss, onClearAll,
}: NotificationPanelProps) {
  useTranslation();
  const panelRef = useRef<HTMLDivElement>(null);

  // Click-outside-to-close
  useEffect(() => {
    if (!isOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        onToggle();
      }
    };
    // Delay to avoid closing immediately from the bell click
    const timer = setTimeout(() => document.addEventListener('mousedown', handleClick), 0);
    return () => { clearTimeout(timer); document.removeEventListener('mousedown', handleClick); };
  }, [isOpen, onToggle]);

  // Focus trap
  useEffect(() => {
    if (!isOpen || !panelRef.current) return;
    const panel = panelRef.current;
    const focusable = panel.querySelectorAll<HTMLElement>('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])');
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    first.focus();

    const handleTab = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { onToggle(); return; }
      if (e.key !== 'Tab') return;
      if (e.shiftKey) {
        if (document.activeElement === first) { e.preventDefault(); last.focus(); }
      } else {
        if (document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    };
    panel.addEventListener('keydown', handleTab);
    return () => panel.removeEventListener('keydown', handleTab);
  }, [isOpen, onToggle]);

  return (
    <>
      {/* Bell Button */}
      <button
        onClick={onToggle}
        style={{
          position: 'relative', background: 'none', border: 'none',
          cursor: 'pointer', color: 'var(--ob-text-secondary)',
          padding: 8, borderRadius: 'var(--ob-radius-sm)',
          transition: 'color 0.2s',
        }}
      >
        <Bell size={20} />
        {unreadCount > 0 && (
          <span style={{
            position: 'absolute', top: 2, right: 2,
            width: 16, height: 16, borderRadius: '50%',
            background: '#ef4444', color: '#fff', fontSize: '0.65rem',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            fontWeight: 700,
          }}>
            {unreadCount > 9 ? '9+' : unreadCount}
          </span>
        )}
      </button>

      {/* Panel */}
      {isOpen && (
        <>
          {/* Backdrop */}
          <div style={{
            position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
            background: 'rgba(0, 0, 0, 0.4)',
            zIndex: 199,
          }} />
          {/* Panel */}
          <div ref={panelRef} style={{
            position: 'fixed', top: 0, right: 0, bottom: 0, width: 360,
            background: 'var(--ob-bg-secondary)', borderLeft: '1px solid var(--ob-glass-border)',
            zIndex: 200, overflow: 'auto',
            animation: 'slideInLeft 0.25s ease both',
            display: 'flex', flexDirection: 'column',
          }}>
            {/* Header */}
            <div style={{
              padding: '16px 20px', borderBottom: '1px solid var(--ob-glass-border)',
              display: 'flex', alignItems: 'center', justifyContent: 'space-between',
            }}>
              <h3 style={{ fontSize: '1rem', fontWeight: 600 }}>
                Notifications {unreadCount > 0 && <span style={{ color: 'var(--ob-accent)', fontSize: '0.85rem' }}>({unreadCount})</span>}
              </h3>
              <div style={{ display: 'flex', gap: 4 }}>
                <button onClick={onMarkAllRead} title="Mark all read" style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-muted)', padding: 4 }}>
                  <CheckCheck size={16} />
                </button>
                <button onClick={onClearAll} title="Clear all" style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-muted)', padding: 4 }}>
                  <Trash2 size={16} />
                </button>
                <button onClick={onToggle} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-muted)', padding: 4 }}>
                  <X size={16} />
                </button>
              </div>
            </div>

            {/* Notification List */}
            <div style={{ flex: 1, overflow: 'auto' }}>
              {notifications.length === 0 ? (
                <div style={{ padding: 40, textAlign: 'center', color: 'var(--ob-text-tertiary)', fontSize: '0.85rem' }}>
                  No notifications
                </div>
              ) : (
                notifications.map(n => {
                  const Icon = ICON_MAP[n.type];
                  const color = COLOR_MAP[n.type];
                  return (
                    <div
                      key={n.id}
                      onClick={() => onMarkRead(n.id)}
                      style={{
                        padding: '12px 20px', cursor: 'pointer',
                        borderBottom: '1px solid var(--ob-glass-border)',
                        background: n.read ? 'transparent' : 'rgba(99, 102, 241, 0.05)',
                        display: 'flex', gap: 12, alignItems: 'flex-start',
                        transition: 'background 0.2s',
                      }}
                    >
                      <Icon size={18} style={{ color, marginTop: 2, flexShrink: 0 }} />
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                          <span style={{ fontWeight: n.read ? 400 : 600, fontSize: '0.85rem' }}>{n.title}</span>
                          <span style={{ fontSize: '0.7rem', color: 'var(--ob-text-tertiary)', whiteSpace: 'nowrap' }}>
                            {timeAgo(n.timestamp)}
                          </span>
                        </div>
                        {n.message && (
                          <p style={{ fontSize: '0.78rem', color: 'var(--ob-text-secondary)', marginTop: 2, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                            {n.message}
                          </p>
                        )}
                      </div>
                      <button
                        onClick={(e) => { e.stopPropagation(); onDismiss(n.id); }}
                        style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-muted)', padding: 2, flexShrink: 0 }}
                      >
                        <X size={14} />
                      </button>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </>
      )}
    </>
  );
}
