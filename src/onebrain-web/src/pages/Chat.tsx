import { useState, useRef, useEffect } from 'react';
import { Link } from 'react-router-dom';
import { Send, Bot, User, Sparkles, Database, BookOpen, ShieldCheck } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import { p10Api, type CognitionStatusView } from '../api/p10Client';
import type { ChatMessage } from '../api/types';
import {
  COVERAGE_LOCAL_STORE_ONLY,
  PROVENANCE_PRIVATE_LOCAL_ONLY,
  USABLE_OFFLINE_BADGE,
  formatModelQualificationBadge,
} from '../utils/epistemicBadges';

export function ChatPage() {
  const { t } = useTranslation();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [cognition, setCognition] = useState<CognitionStatusView | null>(null);
  const [suggestions, setSuggestions] = useState<string[]>([
    'What topics are in my brain?',
    'Summarize my most recent knowledge',
    'Find facts about science',
    'How many KUs do I have?',
  ]);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void p10Api.getCognitionStatus().then(res => setCognition(res.data));
  }, []);

  useEffect(() => {
    bottomRef.current?.scrollIntoView?.({ behavior: 'smooth' });
  }, [messages]);

  const sendMessage = async (overrideText?: string) => {
    const text = (overrideText ?? input).trim();
    if (!text || loading) return;
    const userMsg: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: text,
      timestamp: Date.now(),
    };
    setMessages(prev => [...prev, userMsg]);
    setInput('');
    setLoading(true);

    try {
      const res = await api.chat(text);
      const assistantMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: res.text,
        timestamp: Date.now(),
        kus_encoded: res.kus_encoded || 0,
        kus_retrieved: res.kus_retrieved || 0,
        intent: res.intent,
      };
      setMessages(prev => [...prev, assistantMsg]);
      if (res.suggestions && res.suggestions.length > 0) {
        setSuggestions(res.suggestions);
      }
    } catch {
      setMessages(prev => [
        ...prev,
        {
          id: crypto.randomUUID(),
          role: 'assistant',
          content:
            `${USABLE_OFFLINE_BADGE} · ${PROVENANCE_PRIVATE_LOCAL_ONLY} · ${formatModelQualificationBadge(false)}: ` +
            'Network peers or companion model endpoint are currently unreachable, so this turn stayed inside local Tier M0/M1 PrivateVault memory without contacting external peers. You can continue chatting locally or author structured private KUs in /ku.',
          timestamp: Date.now(),
          intent: 'encode_suggestion',
        },
      ]);
    } finally {
      setLoading(false);
    }
  };

  const handleSuggestion = (suggestion: string) => {
    if (loading) return;
    void sendMessage(suggestion);
  };

  const qualificationBadge = formatModelQualificationBadge(cognition?.model_qualified ?? false);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - var(--ob-header-height))' }}>
      <div
        role="note"
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: 'var(--ob-gap-sm)',
          padding: '8px 16px',
          borderBottom: '1px solid var(--ob-glass-border)',
          background: 'rgba(99, 102, 241, 0.08)',
          color: 'var(--ob-text-secondary)',
          fontSize: '0.8rem',
        }}
      >
        <span>
          Canonical KU creation uses <code>/api/vnext/ku</code> (Draft → Review → Save). Legacy Core DNA encode is disabled in the default build.
        </span>
        <Link to="/ku" style={{ color: '#a5b4fc', fontWeight: 600, textDecoration: 'none' }}>
          Open Local KU Workspace (/api/vnext/ku) →
        </Link>
      </div>

      {/* PAM 4-Tier PrivateVault & Model Qualification Strip (`P6 §4` & `P10 §2`) */}
      <div
        aria-label="PAM PrivateVault and epistemic status"
        style={{
          padding: '8px 16px',
          borderBottom: '1px solid var(--ob-glass-border)',
          background: 'rgba(6, 182, 212, 0.06)',
          fontSize: '0.78rem',
          display: 'flex',
          flexDirection: 'column',
          gap: 6,
        }}
      >
        <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8 }}>
          <ShieldCheck size={14} style={{ color: 'var(--ob-accent-light)' }} />
          <strong>Sovereign PAM · 4-Tier PrivateVault (M0–M3)</strong>
          <span className="badge badge-green" style={{ textTransform: 'none' }}>
            {USABLE_OFFLINE_BADGE}
          </span>
          <span className="badge badge-cyan" style={{ textTransform: 'none' }}>
            {PROVENANCE_PRIVATE_LOCAL_ONLY}
          </span>
          <span className="badge badge-amber" style={{ textTransform: 'none' }}>
            model_qualified: {String(cognition?.model_qualified ?? false)} {qualificationBadge}
          </span>
          <span className="badge badge-violet" style={{ textTransform: 'none' }}>
            {COVERAGE_LOCAL_STORE_ONLY}
          </span>
        </div>
        {cognition && (
          <details style={{ color: 'var(--ob-text-tertiary)' }}>
            <summary style={{ cursor: 'pointer' }}>
              Inspect 4-tier PrivateVault custody ({cognition.pam_storage}) · RAG exposure never emits UseEvidence (NEG-POMV-001)
            </summary>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: 8, marginTop: 6 }}>
              {cognition.tiers.map(tier => (
                <div key={tier.tier} style={{ padding: '6px 8px', borderRadius: 6, background: 'var(--ob-bg-secondary)' }}>
                  <strong>Tier {tier.tier} ({tier.name}):</strong> {tier.custody} — {tier.retention}
                </div>
              ))}
            </div>
          </details>
        )}
      </div>

      {/* Messages */}
      <div style={{ flex: 1, overflow: 'auto', padding: 'var(--ob-gap-lg)' }}>
        {messages.length === 0 && (
          <div className="empty-state" style={{ height: '100%' }}>
            <Bot size={48} style={{ color: 'var(--ob-accent)' }} />
            <h3 style={{ color: 'var(--ob-text-secondary)', fontSize: '1.1rem' }}>{t('chat.title')}</h3>
            <p style={{ fontSize: '0.85rem', maxWidth: 420 }}>
              Ask questions or explore your local brain offline or online. Side-effecting actions remain gated by RecommendationGuard.
            </p>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 16, justifyContent: 'center', maxWidth: 500 }}>
              {suggestions.map((s, i) => (
                <button
                  key={i}
                  onClick={() => handleSuggestion(s)}
                  style={{
                    padding: '8px 16px',
                    borderRadius: 20,
                    background: 'rgba(99, 102, 241, 0.12)',
                    border: '1px solid rgba(99, 102, 241, 0.25)',
                    color: '#a5b4fc',
                    fontSize: '0.82rem',
                    cursor: 'pointer',
                    transition: 'all 0.2s',
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                  }}
                >
                  <Sparkles size={14} /> {s}
                </button>
              ))}
            </div>
          </div>
        )}
        {messages.map(msg => (
          <div
            key={msg.id}
            style={{
              display: 'flex',
              justifyContent: msg.role === 'user' ? 'flex-end' : 'flex-start',
              marginBottom: 'var(--ob-gap-md)',
              animation: 'fadeIn 0.3s ease both',
            }}
          >
            <div
              style={{
                maxWidth: '70%',
                padding: '12px 16px',
                borderRadius:
                  msg.role === 'user'
                    ? 'var(--ob-radius-lg) var(--ob-radius-lg) 4px var(--ob-radius-lg)'
                    : 'var(--ob-radius-lg) var(--ob-radius-lg) var(--ob-radius-lg) 4px',
                background:
                  msg.role === 'user'
                    ? 'linear-gradient(135deg, var(--ob-accent-dark), var(--ob-accent))'
                    : 'var(--ob-glass-strong)',
                border: msg.role === 'user' ? 'none' : '1px solid var(--ob-glass-border)',
                color: 'var(--ob-text-primary)',
                fontSize: '0.9rem',
                lineHeight: 1.6,
              }}
            >
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  marginBottom: 4,
                  fontSize: '0.72rem',
                  color: msg.role === 'user' ? 'rgba(255,255,255,0.7)' : 'var(--ob-text-tertiary)',
                }}
              >
                {msg.role === 'user' ? <User size={12} /> : <Bot size={12} />}
                {msg.role === 'user' ? 'You' : `OneBrain PAM ${qualificationBadge}`}
              </div>
              <div style={{ whiteSpace: 'pre-wrap' }}>{msg.content}</div>
              {msg.role === 'assistant' &&
                (msg.kus_encoded || msg.kus_retrieved || msg.intent === 'encode_suggestion') && (
                  <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 8 }}>
                    {(msg.kus_encoded ?? 0) > 0 && (
                      <span
                        style={{
                          display: 'inline-flex',
                          alignItems: 'center',
                          gap: 4,
                          padding: '4px 10px',
                          borderRadius: 12,
                          fontSize: '0.75rem',
                          fontWeight: 600,
                          background: 'rgba(16, 185, 129, 0.15)',
                          color: '#34d399',
                          border: '1px solid rgba(16, 185, 129, 0.3)',
                        }}
                      >
                        <Database size={12} /> {msg.kus_encoded} KU(s) auto-encoded
                      </span>
                    )}
                    {(msg.kus_retrieved ?? 0) > 0 && (
                      <span
                        style={{
                          display: 'inline-flex',
                          alignItems: 'center',
                          gap: 4,
                          padding: '4px 10px',
                          borderRadius: 12,
                          fontSize: '0.75rem',
                          fontWeight: 500,
                          background: 'rgba(99, 102, 241, 0.12)',
                          color: '#a5b4fc',
                        }}
                      >
                        <BookOpen size={12} /> Referenced {msg.kus_retrieved} KU(s)
                      </span>
                    )}
                    {msg.intent === 'encode_suggestion' && (msg.kus_encoded ?? 0) === 0 && (
                      <Link
                        to="/ku"
                        style={{
                          display: 'inline-flex',
                          alignItems: 'center',
                          gap: 4,
                          padding: '4px 12px',
                          borderRadius: 12,
                          fontSize: '0.75rem',
                          fontWeight: 600,
                          background: 'rgba(245, 158, 11, 0.15)',
                          color: '#fbbf24',
                          border: '1px solid rgba(245, 158, 11, 0.3)',
                          textDecoration: 'none',
                        }}
                      >
                        💡 Open Local KU Workspace (/api/vnext/ku)
                      </Link>
                    )}
                  </div>
                )}
            </div>
          </div>
        ))}
        {loading && (
          <div style={{ display: 'flex', justifyContent: 'flex-start', marginBottom: 'var(--ob-gap-md)' }}>
            <div
              style={{
                padding: '12px 20px',
                borderRadius: 'var(--ob-radius-lg) var(--ob-radius-lg) var(--ob-radius-lg) 4px',
                background: 'var(--ob-glass-strong)',
                border: '1px solid var(--ob-glass-border)',
              }}
            >
              <div className="spinner" />
            </div>
          </div>
        )}
        <div ref={bottomRef} />

        {!loading && messages.length > 0 && suggestions.length > 0 && (
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 8, marginBottom: 16 }}>
            {suggestions.slice(0, 4).map((s, i) => (
              <button
                key={i}
                onClick={() => handleSuggestion(s)}
                style={{
                  padding: '6px 14px',
                  borderRadius: 16,
                  background: 'rgba(99, 102, 241, 0.1)',
                  border: '1px solid rgba(99, 102, 241, 0.2)',
                  color: '#a5b4fc',
                  fontSize: '0.78rem',
                  cursor: 'pointer',
                }}
              >
                {s}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Input Bar — never disabled due to peer offline state (`USABLE_OFFLINE`) */}
      <div
        style={{
          padding: 'var(--ob-gap-md) var(--ob-gap-lg)',
          borderTop: '1px solid var(--ob-glass-border)',
          background: 'rgba(17, 24, 39, 0.8)',
          backdropFilter: 'blur(12px)',
        }}
      >
        <div style={{ display: 'flex', gap: 'var(--ob-gap-sm)', maxWidth: 800, margin: '0 auto' }}>
          <input
            className="input"
            aria-label="Chat message input"
            placeholder="Ask your brain anything (USABLE_OFFLINE)..."
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && !e.shiftKey && void sendMessage()}
            disabled={loading}
          />
          <button
            className="btn btn-primary"
            aria-label="Send message"
            onClick={() => void sendMessage()}
            disabled={loading || !input.trim()}
          >
            <Send size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}
