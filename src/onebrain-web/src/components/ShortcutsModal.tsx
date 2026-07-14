import { X, Keyboard } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { SHORTCUT_LIST } from '../hooks/useKeyboardShortcuts';

type Props = { isOpen: boolean; onClose: () => void };

export function ShortcutsModal({ isOpen, onClose }: Props) {
  const { t } = useTranslation();

  if (!isOpen) return null;

  return (
    <div onClick={onClose} role="dialog" aria-modal="true" aria-label={t('help.keyboardShortcuts')}
      style={{
        position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.6)',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        zIndex: 1000, backdropFilter: 'blur(4px)',
      }}>
      <div onClick={e => e.stopPropagation()} style={{
        background: 'var(--ob-bg-secondary)', borderRadius: 16,
        border: '1px solid var(--ob-glass-border)', width: 520, maxHeight: '80vh',
        overflow: 'auto', boxShadow: '0 24px 64px rgba(0,0,0,0.4)',
      }}>
        {/* Header */}
        <div style={{
          display: 'flex', justifyContent: 'space-between', alignItems: 'center',
          padding: '20px 24px', borderBottom: '1px solid var(--ob-glass-border)',
        }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Keyboard size={20} style={{ color: 'var(--ob-accent)' }} />
            <h2 style={{ margin: 0, fontSize: '1.1rem', fontWeight: 600 }}>{t('help.keyboardShortcuts')}</h2>
          </div>
          <button onClick={onClose} aria-label={t('common.close')}
            style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-secondary)', padding: 4 }}>
            <X size={18} />
          </button>
        </div>

        {/* Body */}
        <div style={{ padding: '16px 24px 24px' }}>
          {SHORTCUT_LIST.map(group => (
            <div key={group.category} style={{ marginBottom: 20 }}>
              <h3 style={{
                fontSize: '0.8rem', fontWeight: 600, textTransform: 'uppercase',
                color: 'var(--ob-text-tertiary)', letterSpacing: '0.05em', marginBottom: 8,
              }}>{group.category}</h3>
              {group.shortcuts.map(sc => (
                <div key={sc.keys} style={{
                  display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                  padding: '8px 0', borderBottom: '1px solid rgba(255,255,255,0.04)',
                }}>
                  <span style={{ fontSize: '0.9rem', color: 'var(--ob-text-primary)' }}>{sc.label}</span>
                  <div style={{ display: 'flex', gap: 4 }}>
                    {sc.keys.split(' + ').map(k => (
                      <kbd key={k} style={{
                        padding: '3px 8px', borderRadius: 5,
                        background: 'var(--ob-bg-tertiary)', border: '1px solid var(--ob-glass-border)',
                        fontSize: '0.78rem', fontFamily: 'var(--ob-font-mono)', fontWeight: 500,
                        color: 'var(--ob-text-secondary)', minWidth: 24, textAlign: 'center',
                        boxShadow: '0 1px 2px rgba(0,0,0,0.2)',
                      }}>{k}</kbd>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
