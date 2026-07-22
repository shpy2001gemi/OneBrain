import { useState, useEffect } from 'react';
import type { StatusResponse } from '../api/types';
import { Server, Cpu, Database, Shield, RefreshCw, Check, AlertCircle, Bell, ArrowRightLeft, Loader2 } from 'lucide-react';
import { api } from '../api/client';
import { formatSize } from '../utils/format';

interface OllamaModel {
  name: string;
  size: number;
  details: {
    parameter_size: string;
    quantization_level: string;
  };
}

interface NotifPrefs {
  encode_complete: boolean;
  peer_connected: boolean;
  sync_complete: boolean;
  watch_match: boolean;
  errors: boolean;
}

const NOTIF_LABELS: { key: keyof NotifPrefs; label: string; desc: string }[] = [
  { key: 'encode_complete', label: 'Encode Complete', desc: 'Notify when encoding finishes' },
  { key: 'peer_connected', label: 'Peer Connected', desc: 'Notify when a new peer connects' },
  { key: 'sync_complete', label: 'Sync Complete', desc: 'Notify when data sync finishes' },
  { key: 'watch_match', label: 'Watch Match', desc: 'Notify when a standing query matches' },
  { key: 'errors', label: 'Errors', desc: 'Notify on system errors' },
];

function NotificationPrefsPanel() {
  const [prefs, setPrefs] = useState<NotifPrefs | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api.getNotificationPrefs().then(setPrefs).catch(() => {});
  }, []);

  const toggle = async (key: keyof NotifPrefs) => {
    if (!prefs) return;
    const updated = { ...prefs, [key]: !prefs[key] };
    setPrefs(updated);
    setSaving(true);
    try {
      await api.setNotificationPrefs(updated);
    } catch { /* revert on error */ setPrefs(prefs); }
    finally { setSaving(false); }
  };

  return (
    <div className="glass-card" style={{ marginTop: 20 }}>
      <h3 style={{ fontWeight: 700, fontSize: '1rem', display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
        <Bell size={18} /> Notification Preferences
        {saving && <span className="spinner" style={{ width: 14, height: 14, marginLeft: 8 }} />}
      </h3>
      {!prefs ? (
        <div style={{ color: 'var(--ob-text-muted)', fontSize: '0.85rem' }}>Loading...</div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {NOTIF_LABELS.map(n => (
            <div key={n.key} style={{
              display: 'flex', justifyContent: 'space-between', alignItems: 'center',
              padding: '10px 14px', borderRadius: 8,
              background: 'var(--ob-surface)', border: '1px solid var(--ob-glass-border)',
            }}>
              <div>
                <div style={{ fontWeight: 600, fontSize: '0.88rem' }}>{n.label}</div>
                <div style={{ fontSize: '0.72rem', color: 'var(--ob-text-muted)' }}>{n.desc}</div>
              </div>
              <button
                onClick={() => toggle(n.key)}
                style={{
                  width: 44, height: 24, borderRadius: 12, border: 'none', cursor: 'pointer',
                  background: prefs[n.key] ? 'var(--ob-accent)' : 'var(--ob-glass-border)',
                  position: 'relative', transition: 'background 0.2s',
                }}
              >
                <div style={{
                  width: 18, height: 18, borderRadius: '50%', background: '#fff',
                  position: 'absolute', top: 3,
                  left: prefs[n.key] ? 23 : 3,
                  transition: 'left 0.2s',
                }} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

export function SettingsPage() {
  const [token, setToken] = useState(localStorage.getItem('ob_api_token') || 'onebrain-dev-token');
  const [ollamaUrl] = useState('http://localhost:11434');
  const [models, setModels] = useState<OllamaModel[]>([]);
  const [currentModel, setCurrentModel] = useState('');
  const [loadedModels, setLoadedModels] = useState<string[]>([]);
  const [status, setStatus] = useState<StatusResponse | null>(null);
  const [msg, setMsg] = useState('');
  const [msgType, setMsgType] = useState<'ok' | 'err'>('ok');
  const [switching, setSwitching] = useState('');

  useEffect(() => {
    fetchStatus();
    fetchModels();
    fetchLoaded();
  }, []);

  const fetchStatus = async () => {
    try {
      const s = await api.getStatus();
      setStatus(s);
      setCurrentModel(s.model || '');
    } catch {}
  };

  const fetchModels = async () => {
    try {
      const r = await fetch(`${ollamaUrl}/api/tags`);
      const j = await r.json();
      setModels(j.models || []);
    } catch {}
  };

  const fetchLoaded = async () => {
    try {
      const r = await fetch(`${ollamaUrl}/api/ps`);
      const j = await r.json();
      setLoadedModels((j.models || []).map((m: { name: string }) => m.name));
    } catch {}
  };

  const unloadModel = async (name: string) => {
    try {
      const res = await fetch(`${ollamaUrl}/api/generate`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model: name, keep_alive: 0 }),
      });
      // Must consume the streaming response body for Ollama to actually unload
      await res.text();
      showMsg(`Unloaded ${name}`, 'ok');
      fetchLoaded();
    } catch {
      showMsg(`Failed to unload ${name}`, 'err');
    }
  };

  const switchToModel = async (name: string) => {
    if (switching || name === currentModel) return;
    setSwitching(name);
    try {
      // Backend handles unloading old models from Ollama and switching config
      await api.switchModel(name);
      setCurrentModel(name);
      await Promise.all([fetchStatus(), fetchModels(), fetchLoaded()]);
      showMsg(`Switched to ${name}`, 'ok');
    } catch {
      showMsg(`Failed to switch to ${name}`, 'err');
    } finally {
      setSwitching('');
    }
  };

  const saveToken = () => {
    localStorage.setItem('ob_api_token', token);
    showMsg('Token saved', 'ok');
  };

  const showMsg = (text: string, type: 'ok' | 'err') => {
    setMsg(text);
    setMsgType(type);
    setTimeout(() => setMsg(''), 3000);
  };

  // formatSize imported from '../utils/format'

  return (
    <div className="page">
      <div className="page-header">
        <h1>Settings</h1>
        <p>Configure your OneBrain node</p>
      </div>

      {msg && (
        <div style={{
          padding: '10px 16px', borderRadius: 8, marginBottom: 16,
          background: msgType === 'ok' ? 'rgba(76, 175, 80, 0.15)' : 'rgba(239, 68, 68, 0.15)',
          color: msgType === 'ok' ? 'var(--ob-success)' : 'var(--ob-error)',
          display: 'flex', alignItems: 'center', gap: 8,
        }}>
          {msgType === 'ok' ? <Check size={16} /> : <AlertCircle size={16} />}
          {msg}
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 'var(--ob-gap-lg)' }}>
        {/* Node Info */}
        <div className="glass-card animate-in">
          <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 16, display: 'flex', alignItems: 'center', gap: 8 }}>
            <Server size={18} style={{ color: 'var(--ob-accent)' }} /> Node Info
          </h3>
          {status && (
            <div style={{ fontSize: '0.85rem', display: 'flex', flexDirection: 'column', gap: 10 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--ob-text-tertiary)' }}>Name</span>
                <span>{status.node_name}</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--ob-text-tertiary)' }}>Version</span>
                <span>{status.version || 'dev'}</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--ob-text-tertiary)' }}>KU Count</span>
                <span style={{ color: 'var(--ob-accent)' }}>{status.ku_count}</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--ob-text-tertiary)' }}>Peers</span>
                <span>{status.peer_count}</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--ob-text-tertiary)' }}>Model</span>
                <span style={{ color: 'var(--ob-warning)' }}>{currentModel}</span>
              </div>
            </div>
          )}
        </div>

        {/* API Token */}
        <div className="glass-card animate-in">
          <h3 style={{ fontSize: '1rem', fontWeight: 600, marginBottom: 16, display: 'flex', alignItems: 'center', gap: 8 }}>
            <Shield size={18} style={{ color: 'var(--ob-accent)' }} /> API Token
          </h3>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <input
              className="input"
              type="password"
              value={token}
              onChange={e => setToken(e.target.value)}
              placeholder="API token"
            />
            <button className="btn btn-primary" onClick={saveToken}>
              Save Token
            </button>
            <p style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)' }}>
              Used for authenticating API and WebSocket requests.
            </p>
          </div>
        </div>

        {/* Ollama Models */}
        <div className="glass-card animate-in" style={{ gridColumn: '1 / -1' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
            <h3 style={{ fontSize: '1rem', fontWeight: 600, display: 'flex', alignItems: 'center', gap: 8 }}>
              <Cpu size={18} style={{ color: 'var(--ob-accent)' }} /> Ollama Models
            </h3>
            <button className="btn btn-ghost" onClick={() => { fetchModels(); fetchLoaded(); }}>
              <RefreshCw size={14} /> Refresh
            </button>
          </div>

          {models.length === 0 ? (
            <p style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.85rem' }}>
              No models found. Is Ollama running at {ollamaUrl}?
            </p>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              {models.map(m => {
                const isLoaded = loadedModels.includes(m.name);
                const isCurrent = m.name === currentModel;
                return (
                  <div key={m.name} style={{
                    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
                    padding: '10px 14px', borderRadius: 8,
                    background: isCurrent ? 'rgba(79, 195, 247, 0.08)' : 'rgba(255,255,255,0.03)',
                    border: isCurrent ? '1px solid rgba(79, 195, 247, 0.3)' : '1px solid rgba(255,255,255,0.06)',
                  }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                      <Database size={16} style={{ color: isCurrent ? 'var(--ob-accent)' : 'var(--ob-text-tertiary)' }} />
                      <div>
                        <div style={{ fontWeight: 600, fontSize: '0.9rem' }}>
                          {m.name}
                          {isCurrent && <span style={{ color: 'var(--ob-accent)', fontSize: '0.75rem', marginLeft: 8 }}>● ACTIVE</span>}
                        </div>
                        <div style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)' }}>
                          {m.details.parameter_size} · {m.details.quantization_level} · {formatSize(m.size)}
                        </div>
                      </div>
                    </div>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      {isLoaded && (
                        <>
                          <span style={{ fontSize: '0.72rem', color: 'var(--ob-success)', fontWeight: 600 }}>LOADED</span>
                          <button className="btn btn-ghost" style={{ fontSize: '0.75rem', padding: '4px 10px' }} onClick={() => unloadModel(m.name)}>
                            Unload
                          </button>
                        </>
                      )}
                      {!isCurrent && (
                        <button
                          className="btn"
                          disabled={!!switching}
                          onClick={() => switchToModel(m.name)}
                          style={{
                            fontSize: '0.75rem', padding: '4px 12px',
                            background: switching === m.name ? 'var(--ob-glass-border)' : 'var(--ob-accent)',
                            color: '#000', fontWeight: 600, border: 'none', borderRadius: 6,
                            cursor: switching ? 'not-allowed' : 'pointer',
                            display: 'flex', alignItems: 'center', gap: 4,
                            opacity: switching && switching !== m.name ? 0.5 : 1,
                          }}
                        >
                          {switching === m.name ? (
                            <><Loader2 size={12} className="spin" /> Switching...</>
                          ) : (
                            <><ArrowRightLeft size={12} /> Use this</>
                          )}
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
          <p style={{ fontSize: '0.72rem', color: 'var(--ob-text-tertiary)', marginTop: 12 }}>
            💡 Click "Use this" to switch model. The current model will be unloaded automatically.
          </p>
        </div>

        {/* Notification Preferences */}
        <NotificationPrefsPanel />
      </div>
    </div>
  );
}
