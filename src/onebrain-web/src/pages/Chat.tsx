import { useState, useRef, useEffect } from 'react';
import { Send, Bot, User, Sparkles } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import type { ChatMessage } from '../api/types';

export function ChatPage() {
  const { t } = useTranslation();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const [suggestions, setSuggestions] = useState<string[]>([
    'What topics are in my brain?',
    'Summarize my most recent knowledge',
    'Find facts about science',
    'How many KUs do I have?',
  ]);
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const sendMessage = async () => {
    if (!input.trim() || loading) return;
    const userMsg: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: input.trim(),
      timestamp: Date.now(),
    };
    setMessages(prev => [...prev, userMsg]);
    setInput('');
    setLoading(true);

    try {
      const res = await api.chat(input.trim());
      const assistantMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: res.text,
        timestamp: Date.now(),
      };
      setMessages(prev => [...prev, assistantMsg]);
      // Update suggestions from response
      if (res.suggestions && res.suggestions.length > 0) {
        setSuggestions(res.suggestions);
      }
    } catch (err: any) {
      setMessages(prev => [...prev, {
        id: crypto.randomUUID(),
        role: 'assistant',
        content: `⚠️ Error: ${err.message || 'Failed to get response'}`,
        timestamp: Date.now(),
      }]);
    } finally {
      setLoading(false);
    }
  };

  const handleSuggestion = (suggestion: string) => {
    setInput(suggestion);
    // Auto-send after tiny delay so UI updates
    setTimeout(() => {
      setInput('');
      const userMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: 'user',
        content: suggestion,
        timestamp: Date.now(),
      };
      setMessages(prev => [...prev, userMsg]);
      setLoading(true);
      api.chat(suggestion).then(res => {
        setMessages(prev => [...prev, {
          id: crypto.randomUUID(),
          role: 'assistant',
          content: res.text,
          timestamp: Date.now(),
        }]);
        if (res.suggestions?.length) setSuggestions(res.suggestions);
      }).catch(err => {
        setMessages(prev => [...prev, {
          id: crypto.randomUUID(),
          role: 'assistant',
          content: `⚠️ Error: ${err.message || 'Failed'}`,
          timestamp: Date.now(),
        }]);
      }).finally(() => setLoading(false));
    }, 50);
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - var(--ob-header-height))' }}>
      {/* Messages */}
      <div style={{ flex: 1, overflow: 'auto', padding: 'var(--ob-gap-lg)' }}>
        {messages.length === 0 && (
          <div className="empty-state" style={{ height: '100%' }}>
            <Bot size={48} style={{ color: 'var(--ob-accent)' }} />
            <h3 style={{ color: 'var(--ob-text-secondary)', fontSize: '1.1rem' }}>{t('chat.title')}</h3>
            <p style={{ fontSize: '0.85rem', maxWidth: 400 }}>
              Ask questions, encode knowledge, or explore your brain through natural conversation.
            </p>
            {/* Suggestion chips for empty state */}
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 16, justifyContent: 'center', maxWidth: 500 }}>
              {suggestions.map((s, i) => (
                <button
                  key={i}
                  onClick={() => handleSuggestion(s)}
                  style={{
                    padding: '8px 16px', borderRadius: 20,
                    background: 'rgba(99, 102, 241, 0.12)', border: '1px solid rgba(99, 102, 241, 0.25)',
                    color: '#a5b4fc', fontSize: '0.82rem', cursor: 'pointer',
                    transition: 'all 0.2s', display: 'flex', alignItems: 'center', gap: 6,
                  }}
                  onMouseEnter={e => { e.currentTarget.style.background = 'rgba(99, 102, 241, 0.25)'; }}
                  onMouseLeave={e => { e.currentTarget.style.background = 'rgba(99, 102, 241, 0.12)'; }}
                >
                  <Sparkles size={14} /> {s}
                </button>
              ))}
            </div>
          </div>
        )}
        {messages.map(msg => (
          <div key={msg.id} style={{
            display: 'flex',
            justifyContent: msg.role === 'user' ? 'flex-end' : 'flex-start',
            marginBottom: 'var(--ob-gap-md)',
            animation: 'fadeIn 0.3s ease both',
          }}>
            <div style={{
              maxWidth: '70%',
              padding: '12px 16px',
              borderRadius: msg.role === 'user'
                ? 'var(--ob-radius-lg) var(--ob-radius-lg) 4px var(--ob-radius-lg)'
                : 'var(--ob-radius-lg) var(--ob-radius-lg) var(--ob-radius-lg) 4px',
              background: msg.role === 'user'
                ? 'linear-gradient(135deg, var(--ob-accent-dark), var(--ob-accent))'
                : 'var(--ob-glass-strong)',
              border: msg.role === 'user' ? 'none' : '1px solid var(--ob-glass-border)',
              color: 'var(--ob-text-primary)',
              fontSize: '0.9rem',
              lineHeight: 1.6,
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4, fontSize: '0.72rem', color: msg.role === 'user' ? 'rgba(255,255,255,0.7)' : 'var(--ob-text-tertiary)' }}>
                {msg.role === 'user' ? <User size={12} /> : <Bot size={12} />}
                {msg.role === 'user' ? 'You' : 'OneBrain'}
              </div>
              <div style={{ whiteSpace: 'pre-wrap' }}>{msg.content}</div>
            </div>
          </div>
        ))}
        {loading && (
          <div style={{ display: 'flex', justifyContent: 'flex-start', marginBottom: 'var(--ob-gap-md)' }}>
            <div style={{
              padding: '12px 20px',
              borderRadius: 'var(--ob-radius-lg) var(--ob-radius-lg) var(--ob-radius-lg) 4px',
              background: 'var(--ob-glass-strong)',
              border: '1px solid var(--ob-glass-border)',
            }}>
              <div className="spinner" />
            </div>
          </div>
        )}
        <div ref={bottomRef} />

        {/* Suggestion chips after messages */}
        {!loading && messages.length > 0 && suggestions.length > 0 && (
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 8, marginBottom: 16 }}>
            {suggestions.slice(0, 4).map((s, i) => (
              <button
                key={i}
                onClick={() => handleSuggestion(s)}
                style={{
                  padding: '6px 14px', borderRadius: 16,
                  background: 'rgba(99, 102, 241, 0.1)', border: '1px solid rgba(99, 102, 241, 0.2)',
                  color: '#a5b4fc', fontSize: '0.78rem', cursor: 'pointer',
                  transition: 'all 0.2s',
                }}
                onMouseEnter={e => { e.currentTarget.style.background = 'rgba(99, 102, 241, 0.2)'; }}
                onMouseLeave={e => { e.currentTarget.style.background = 'rgba(99, 102, 241, 0.1)'; }}
              >
                {s}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Input Bar */}
      <div style={{
        padding: 'var(--ob-gap-md) var(--ob-gap-lg)',
        borderTop: '1px solid var(--ob-glass-border)',
        background: 'rgba(17, 24, 39, 0.8)',
        backdropFilter: 'blur(12px)',
      }}>
        <div style={{ display: 'flex', gap: 'var(--ob-gap-sm)', maxWidth: 800, margin: '0 auto' }}>
          <input
            className="input"
            placeholder="Ask your brain anything..."
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && !e.shiftKey && sendMessage()}
            disabled={loading}
          />
          <button
            className="btn btn-primary"
            onClick={sendMessage}
            disabled={loading || !input.trim()}
          >
            <Send size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}
