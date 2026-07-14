import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Brain, ArrowRight, ArrowLeft, Check, Settings, Globe2, Cpu, Zap } from 'lucide-react';
import { api } from '../api/client';

interface SetupWizardProps {
  onComplete: () => void;
}

export function SetupWizard({ onComplete }: SetupWizardProps) {
  const { t } = useTranslation();
  const [step, setStep] = useState(0);
  const [config, setConfig] = useState({
    nodeName: '',
    dataDir: './data',
    ollamaUrl: 'http://localhost:11434',
    model: 'llama3.2',
    seeds: '',
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');

  const steps = [
    { icon: Brain, title: 'Welcome', description: 'Set up your OneBrain node' },
    { icon: Settings, title: 'Node Configuration', description: 'Name and data directory' },
    { icon: Cpu, title: 'AI Engine', description: 'Connect to your AI model' },
    { icon: Globe2, title: 'Network', description: 'Connect to the network' },
    { icon: Zap, title: 'Ready!', description: 'Your brain is configured' },
  ];

  const handleSave = async () => {
    setSaving(true);
    setError('');
    try {
      await api.updateConfig({
        node_name: config.nodeName || undefined,
        data_dir: config.dataDir || undefined,
        ollama_url: config.ollamaUrl || undefined,
        model: config.model || undefined,
        seeds: config.seeds ? config.seeds.split(',').map(s => s.trim()) : undefined,
      });
      localStorage.setItem('ob_setup_complete', 'true');
      onComplete();
    } catch (err: any) {
      setError(err.message || 'Failed to save configuration');
      setSaving(false);
    }
  };

  const canNext = () => {
    if (step === 1) return config.nodeName.trim().length > 0;
    return true;
  };

  return (
    <div style={{
      position: 'fixed', inset: 0, zIndex: 1000,
      background: 'var(--ob-bg-primary)',
      display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
    }}>
      {/* Progress */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 40 }}>
        {steps.map((s, i) => (
          <div key={i} style={{
            width: 40, height: 4, borderRadius: 2,
            background: i <= step ? 'var(--ob-accent)' : 'var(--ob-glass-border)',
            transition: 'background 0.3s',
          }} />
        ))}
      </div>

      {/* Card */}
      <div className="glass-card" style={{
        width: 520, maxWidth: '90vw',
        padding: 'var(--ob-gap-xl)',
        animation: 'fadeIn 0.4s ease both',
      }}>
        {/* Step Icon */}
        <div style={{ textAlign: 'center', marginBottom: 24 }}>
          {(() => {
            const Icon = steps[step].icon;
            return <Icon size={48} style={{ color: 'var(--ob-accent)', marginBottom: 12 }} />;
          })()}
          <h2 style={{ fontSize: '1.4rem', fontWeight: 700, marginBottom: 8 }}>{steps[step].title}</h2>
          <p style={{ color: 'var(--ob-text-secondary)', fontSize: '0.9rem' }}>{steps[step].description}</p>
        </div>

        {/* Step Content */}
        <div style={{ marginBottom: 32 }}>
          {step === 0 && (
            <div style={{ textAlign: 'center', color: 'var(--ob-text-secondary)', lineHeight: 1.8 }}>
              <p>Welcome to <strong style={{ color: 'var(--ob-accent-light)' }}>OneBrain</strong> — your personal knowledge management system powered by biological encoding.</p>
              <p style={{ marginTop: 12, fontSize: '0.85rem' }}>This wizard will help you configure your node in a few quick steps.</p>
            </div>
          )}

          {step === 1 && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
              <div>
                <label style={{ display: 'block', fontSize: '0.82rem', color: 'var(--ob-text-secondary)', marginBottom: 6 }}>
                  {t('settings.nodeName')} *
                </label>
                <input
                  className="input"
                  placeholder="My Brain"
                  value={config.nodeName}
                  onChange={e => setConfig({ ...config, nodeName: e.target.value })}
                  autoFocus
                />
              </div>
              <div>
                <label style={{ display: 'block', fontSize: '0.82rem', color: 'var(--ob-text-secondary)', marginBottom: 6 }}>
                  {t('settings.dataDir')}
                </label>
                <input
                  className="input"
                  placeholder="./data"
                  value={config.dataDir}
                  onChange={e => setConfig({ ...config, dataDir: e.target.value })}
                />
              </div>
            </div>
          )}

          {step === 2 && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
              <div>
                <label style={{ display: 'block', fontSize: '0.82rem', color: 'var(--ob-text-secondary)', marginBottom: 6 }}>
                  {t('settings.ollamaUrl')}
                </label>
                <input
                  className="input"
                  placeholder="http://localhost:11434"
                  value={config.ollamaUrl}
                  onChange={e => setConfig({ ...config, ollamaUrl: e.target.value })}
                />
              </div>
              <div>
                <label style={{ display: 'block', fontSize: '0.82rem', color: 'var(--ob-text-secondary)', marginBottom: 6 }}>
                  {t('settings.model')}
                </label>
                <input
                  className="input"
                  placeholder="llama3.2"
                  value={config.model}
                  onChange={e => setConfig({ ...config, model: e.target.value })}
                />
              </div>
            </div>
          )}

          {step === 3 && (
            <div>
              <label style={{ display: 'block', fontSize: '0.82rem', color: 'var(--ob-text-secondary)', marginBottom: 6 }}>
                {t('settings.seeds')} (comma-separated)
              </label>
              <input
                className="input"
                placeholder="/ip4/104.131.131.82/tcp/4001/p2p/QmaCpD..."
                value={config.seeds}
                onChange={e => setConfig({ ...config, seeds: e.target.value })}
              />
              <p style={{ fontSize: '0.78rem', color: 'var(--ob-text-tertiary)', marginTop: 8 }}>
                Optional: Add bootstrap nodes to connect to the OneBrain network.
              </p>
            </div>
          )}

          {step === 4 && (
            <div style={{ textAlign: 'center' }}>
              <div style={{
                width: 64, height: 64, borderRadius: '50%',
                background: 'linear-gradient(135deg, var(--ob-success), #065f46)',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                margin: '0 auto 16px',
              }}>
                <Check size={32} style={{ color: '#fff' }} />
              </div>
              <p style={{ color: 'var(--ob-text-secondary)', lineHeight: 1.8 }}>
                Your OneBrain node <strong style={{ color: 'var(--ob-accent-light)' }}>{config.nodeName || 'Unnamed'}</strong> is ready to go!
              </p>

              {/* Config summary */}
              <div style={{
                marginTop: 16, padding: 16, borderRadius: 'var(--ob-radius-md)',
                background: 'var(--ob-surface)', textAlign: 'left',
                fontSize: '0.82rem', color: 'var(--ob-text-secondary)',
              }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
                  <span>Node</span><span className="mono" style={{ color: 'var(--ob-accent-light)' }}>{config.nodeName}</span>
                </div>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
                  <span>AI</span><span className="mono">{config.model}@{config.ollamaUrl}</span>
                </div>
                <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <span>Data</span><span className="mono">{config.dataDir}</span>
                </div>
              </div>
            </div>
          )}
        </div>

        {error && (
          <p style={{ color: '#f87171', fontSize: '0.82rem', marginBottom: 12 }}>{error}</p>
        )}

        {/* Navigation */}
        <div style={{ display: 'flex', justifyContent: 'space-between' }}>
          {step > 0 ? (
            <button className="btn" onClick={() => setStep(s => s - 1)}>
              <ArrowLeft size={16} /> {t('common.back')}
            </button>
          ) : <div />}

          {step < steps.length - 1 ? (
            <button
              className="btn btn-primary"
              onClick={() => setStep(s => s + 1)}
              disabled={!canNext()}
            >
              {t('common.next')} <ArrowRight size={16} />
            </button>
          ) : (
            <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
              {saving ? <span className="spinner" /> : <><Zap size={16} /> Start Using OneBrain</>}
            </button>
          )}
        </div>
      </div>

      {/* Skip link */}
      <button
        onClick={() => { localStorage.setItem('ob_setup_complete', 'true'); onComplete(); }}
        style={{
          marginTop: 24, background: 'none', border: 'none',
          color: 'var(--ob-text-tertiary)', cursor: 'pointer',
          fontSize: '0.82rem', textDecoration: 'underline',
        }}
      >
        Skip setup
      </button>
    </div>
  );
}
