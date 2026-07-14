import { useState } from 'react';
import { HelpCircle, BookOpen, Keyboard, MessageSquare, Bug, ChevronRight, ExternalLink, Send } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { SHORTCUT_LIST } from '../hooks/useKeyboardShortcuts';

type Tab = 'guide' | 'shortcuts' | 'feedback';

export function HelpPage() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>('guide');
  const [feedbackType, setFeedbackType] = useState<'feedback' | 'bug'>('feedback');
  const [feedbackText, setFeedbackText] = useState('');
  const [submitted, setSubmitted] = useState(false);

  const tabs = [
    { key: 'guide' as const, icon: BookOpen, label: t('help.guide') },
    { key: 'shortcuts' as const, icon: Keyboard, label: t('help.keyboardShortcuts') },
    { key: 'feedback' as const, icon: MessageSquare, label: t('help.feedback') },
  ];

  const handleSubmitFeedback = () => {
    if (!feedbackText.trim()) return;
    // Store locally (will be sent when backend supports it)
    const existing = JSON.parse(localStorage.getItem('ob_feedback') || '[]');
    existing.push({ type: feedbackType, text: feedbackText.trim(), timestamp: Date.now() });
    localStorage.setItem('ob_feedback', JSON.stringify(existing));
    setFeedbackText('');
    setSubmitted(true);
    setTimeout(() => setSubmitted(false), 3000);
  };

  return (
    <div className="page">
      <div className="page-header">
        <h1><HelpCircle size={28} style={{ display: 'inline', marginRight: 8, verticalAlign: 'middle' }} />{t('help.title')}</h1>
      </div>

      {/* Tab Bar */}
      <div style={{
        display: 'flex', gap: 4, padding: 4,
        background: 'var(--ob-bg-secondary)', borderRadius: 12,
        marginBottom: 24, width: 'fit-content',
      }}>
        {tabs.map(({ key, icon: Icon, label }) => (
          <button key={key} onClick={() => setTab(key)}
            style={{
              display: 'flex', alignItems: 'center', gap: 8,
              padding: '10px 20px', borderRadius: 10, border: 'none', cursor: 'pointer',
              background: tab === key ? 'var(--ob-accent)' : 'transparent',
              color: tab === key ? '#fff' : 'var(--ob-text-secondary)',
              fontWeight: tab === key ? 600 : 400, fontSize: '0.9rem',
              transition: 'all var(--ob-transition)',
            }}>
            <Icon size={16} />
            {label}
          </button>
        ))}
      </div>

      {/* ── Guide Tab ─── */}
      {tab === 'guide' && (
        <div style={{ display: 'grid', gap: 16 }}>
          {[
            { title: t('help.gettingStarted'), desc: t('help.gettingStartedDesc'), icon: '🚀' },
            { title: t('help.encodingKnowledge'), desc: t('help.encodingDesc'), icon: '⚡' },
            { title: t('help.exploringData'), desc: t('help.exploringDesc'), icon: '🔍' },
            { title: t('help.graphVisualization'), desc: t('help.graphDesc'), icon: '🕸️' },
            { title: t('help.socialFeatures'), desc: t('help.socialDesc'), icon: '👥' },
            { title: t('help.walletAndTokens'), desc: t('help.walletDesc'), icon: '💰' },
          ].map(item => (
            <div key={item.title} className="glass-card" style={{
              padding: '18px 22px', borderRadius: 12, background: 'var(--ob-bg-tertiary)',
              display: 'flex', alignItems: 'center', gap: 16, cursor: 'pointer',
              transition: 'all var(--ob-transition)',
            }}>
              <div style={{ fontSize: '1.5rem', flexShrink: 0 }}>{item.icon}</div>
              <div style={{ flex: 1 }}>
                <div style={{ fontSize: '1rem', fontWeight: 600, color: 'var(--ob-text-primary)', marginBottom: 4 }}>{item.title}</div>
                <div style={{ fontSize: '0.85rem', color: 'var(--ob-text-secondary)' }}>{item.desc}</div>
              </div>
              <ChevronRight size={18} style={{ color: 'var(--ob-text-tertiary)', flexShrink: 0 }} />
            </div>
          ))}

          {/* Version Info */}
          <div style={{ marginTop: 20, padding: 20, borderRadius: 12, background: 'var(--ob-bg-tertiary)', textAlign: 'center' }}>
            <div style={{ fontSize: '0.85rem', color: 'var(--ob-text-tertiary)' }}>OneBrain v0.1.0 · KUv7 Architecture</div>
            <div style={{ fontSize: '0.78rem', color: 'var(--ob-text-tertiary)', marginTop: 4 }}>
              <a href="https://github.com/onebrain" target="_blank" rel="noopener noreferrer"
                style={{ color: 'var(--ob-accent)', textDecoration: 'none', display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                Documentation <ExternalLink size={12} />
              </a>
            </div>
          </div>
        </div>
      )}

      {/* ── Shortcuts Tab ─── */}
      {tab === 'shortcuts' && (
        <div className="glass-card" style={{ padding: '20px 24px', borderRadius: 12, background: 'var(--ob-bg-tertiary)' }}>
          {SHORTCUT_LIST.map(group => (
            <div key={group.category} style={{ marginBottom: 24 }}>
              <h3 style={{
                fontSize: '0.8rem', fontWeight: 600, textTransform: 'uppercase',
                color: 'var(--ob-text-tertiary)', letterSpacing: '0.05em', marginBottom: 10,
              }}>{group.category}</h3>
              {group.shortcuts.map(sc => (
                <div key={sc.keys} style={{
                  display: 'flex', justifyContent: 'space-between', alignItems: 'center',
                  padding: '10px 0', borderBottom: '1px solid rgba(255,255,255,0.04)',
                }}>
                  <span style={{ fontSize: '0.9rem', color: 'var(--ob-text-primary)' }}>{sc.label}</span>
                  <div style={{ display: 'flex', gap: 4 }}>
                    {sc.keys.split(' + ').map(k => (
                      <kbd key={k} style={{
                        padding: '3px 8px', borderRadius: 5,
                        background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)',
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
      )}

      {/* ── Feedback Tab ─── */}
      {tab === 'feedback' && (
        <div className="glass-card" style={{ padding: '24px', borderRadius: 12, background: 'var(--ob-bg-tertiary)', maxWidth: 600 }}>
          <div style={{ display: 'flex', gap: 8, marginBottom: 20 }}>
            <button onClick={() => setFeedbackType('feedback')}
              style={{
                display: 'flex', alignItems: 'center', gap: 6, padding: '8px 16px', borderRadius: 8,
                border: 'none', cursor: 'pointer',
                background: feedbackType === 'feedback' ? 'var(--ob-accent)' : 'var(--ob-bg-secondary)',
                color: feedbackType === 'feedback' ? '#fff' : 'var(--ob-text-secondary)',
                fontWeight: feedbackType === 'feedback' ? 600 : 400, fontSize: '0.88rem',
              }}>
              <MessageSquare size={14} />{t('help.sendFeedback')}
            </button>
            <button onClick={() => setFeedbackType('bug')}
              style={{
                display: 'flex', alignItems: 'center', gap: 6, padding: '8px 16px', borderRadius: 8,
                border: 'none', cursor: 'pointer',
                background: feedbackType === 'bug' ? '#ef4444' : 'var(--ob-bg-secondary)',
                color: feedbackType === 'bug' ? '#fff' : 'var(--ob-text-secondary)',
                fontWeight: feedbackType === 'bug' ? 600 : 400, fontSize: '0.88rem',
              }}>
              <Bug size={14} />{t('help.reportBug')}
            </button>
          </div>

          <textarea
            value={feedbackText}
            onChange={e => setFeedbackText(e.target.value)}
            placeholder={feedbackType === 'bug' ? t('help.bugPlaceholder') : t('help.feedbackPlaceholder')}
            rows={6}
            aria-label={feedbackType === 'bug' ? t('help.reportBug') : t('help.sendFeedback')}
            style={{
              width: '100%', padding: '14px', borderRadius: 10,
              background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)',
              color: 'var(--ob-text-primary)', fontSize: '0.9rem', resize: 'vertical',
              fontFamily: 'inherit', lineHeight: 1.5,
            }}
          />

          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 12 }}>
            <span style={{ fontSize: '0.8rem', color: submitted ? 'var(--ob-success)' : 'var(--ob-text-tertiary)' }}>
              {submitted ? '✅ ' + t('help.thankYou') : t('help.storedLocally')}
            </span>
            <button onClick={handleSubmitFeedback} className="btn-primary"
              disabled={!feedbackText.trim()}
              style={{
                display: 'flex', alignItems: 'center', gap: 6,
                padding: '10px 24px', borderRadius: 10,
                opacity: feedbackText.trim() ? 1 : 0.5,
              }}>
              <Send size={14} />{t('common.submit')}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
