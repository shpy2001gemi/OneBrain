import { Wifi, WifiOff, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';

type Props = {
  status: 'connected' | 'connecting' | 'disconnected';
  lastPing: number;
  retryCount: number;
  onRetry: () => void;
};

export function ConnectionBar({ status, lastPing, retryCount, onRetry }: Props) {
  const { t } = useTranslation();

  if (status === 'connected') return null;

  const isConnecting = status === 'connecting';

  return (
    <div role="alert" aria-live="polite" style={{
      background: isConnecting ? 'linear-gradient(90deg, #f59e0b20, #f59e0b10)' : 'linear-gradient(90deg, #ef444420, #ef444410)',
      borderBottom: `2px solid ${isConnecting ? '#f59e0b' : '#ef4444'}`,
      padding: '8px 20px',
      display: 'flex', alignItems: 'center', justifyContent: 'space-between',
      fontSize: '0.85rem',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        <WifiOff size={16} style={{ color: isConnecting ? '#f59e0b' : '#ef4444' }} />
        <span style={{ color: isConnecting ? '#f59e0b' : '#ef4444', fontWeight: 500 }}>
          {isConnecting ? t('network.connecting') : t('network.disconnected')}
        </span>
        {retryCount > 0 && (
          <span style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.78rem' }}>
            (retry #{retryCount})
          </span>
        )}
      </div>
      <button onClick={onRetry} aria-label={t('network.retry')}
        style={{
          display: 'flex', alignItems: 'center', gap: 4,
          background: 'none', border: '1px solid var(--ob-glass-border)',
          borderRadius: 6, padding: '4px 12px', cursor: 'pointer',
          color: 'var(--ob-text-secondary)', fontSize: '0.82rem',
        }}>
        <RefreshCw size={12} />
        {t('network.retry')}
      </button>
    </div>
  );
}
