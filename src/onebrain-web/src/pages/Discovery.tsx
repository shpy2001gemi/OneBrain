import { useEffect, useState } from 'react';
import { TrendingUp, Lightbulb, Layers, ChevronRight, Flame, Star, Sparkles } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import type { KuListItem } from '../api/types';
import { GENE_TYPE_COLORS } from '../api/types';

type TrendingItem = { ku: KuListItem; trend_score: number; reason: string };
type RecommendedItem = { ku: KuListItem; relevance: number; reason: string };
type DomainItem = { name: string; ku_count: number; avg_pomv: number; example_cids: string[] };

const REASON_LABELS: Record<string, string> = {
  high_pomv: 'High PoMV',
  recently_encoded: 'Recently Encoded',
  steady_quality: 'Steady Quality',
  needs_review: 'Needs Review',
  matches_interest: 'Matches Interest',
  discover_new_type: 'Discover New Type',
};

const REASON_COLORS: Record<string, string> = {
  high_pomv: '#f59e0b',
  recently_encoded: '#10b981',
  steady_quality: '#6366f1',
  needs_review: '#ef4444',
  matches_interest: '#3b82f6',
  discover_new_type: '#8b5cf6',
};

export function DiscoveryPage() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<'trending' | 'recommended' | 'domains'>('trending');
  const [trending, setTrending] = useState<TrendingItem[]>([]);
  const [recommended, setRecommended] = useState<RecommendedItem[]>([]);
  const [domains, setDomains] = useState<DomainItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedDomain, setSelectedDomain] = useState<string | null>(null);
  const [domainKus, setDomainKus] = useState<KuListItem[]>([]);

  useEffect(() => {
    setLoading(true);
    if (tab === 'trending') {
      api.getTrending(20).then(d => setTrending(d.trending || [])).catch(() => setTrending([])).finally(() => setLoading(false));
    } else if (tab === 'recommended') {
      api.getRecommendations(20).then(d => setRecommended(d.recommendations || [])).catch(() => setRecommended([])).finally(() => setLoading(false));
    } else {
      api.listDomains().then(d => setDomains(d.domains || [])).catch(() => setDomains([])).finally(() => setLoading(false));
    }
  }, [tab]);

  const loadDomainKus = (domain: string) => {
    setSelectedDomain(domain);
    api.kusByDomain(domain, 1, 50).then(d => setDomainKus(d.kus || [])).catch(() => setDomainKus([]));
  };

  const tabs = [
    { key: 'trending' as const, icon: Flame, label: t('discovery.trending') },
    { key: 'recommended' as const, icon: Lightbulb, label: t('discovery.recommended') },
    { key: 'domains' as const, icon: Layers, label: t('discovery.domains') },
  ];

  return (
    <div className="page">
      <div className="page-header">
        <h1><Sparkles size={28} style={{ display: 'inline', marginRight: 8, verticalAlign: 'middle' }} />{t('discovery.title')}</h1>
      </div>

      {/* Tab Bar */}
      <div style={{
        display: 'flex', gap: 4, padding: '4px',
        background: 'var(--ob-bg-secondary)', borderRadius: 12,
        marginBottom: 24, width: 'fit-content',
      }}>
        {tabs.map(({ key, icon: Icon, label }) => (
          <button key={key} onClick={() => { setTab(key); setSelectedDomain(null); }}
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

      {loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 60 }}>
          <div className="spinner spinner-lg" />
        </div>
      ) : (
        <>
          {/* ── Trending ─── */}
          {tab === 'trending' && (
            trending.length === 0 ? (
              <EmptyState icon={<Flame size={48} />} message={t('discovery.noTrending')} />
            ) : (
              <div style={{ display: 'grid', gap: 12 }}>
                {trending.map((item, i) => (
                  <KuCard key={item.ku.cid_hex} ku={item.ku} rank={i + 1}
                    badge={<ScoreBadge label={t('discovery.trendScore')} value={item.trend_score} />}
                    reason={item.reason} />
                ))}
              </div>
            )
          )}

          {/* ── Recommended ─── */}
          {tab === 'recommended' && (
            recommended.length === 0 ? (
              <EmptyState icon={<Lightbulb size={48} />} message={t('discovery.noRecommendations')} />
            ) : (
              <div style={{ display: 'grid', gap: 12 }}>
                {recommended.map((item, i) => (
                  <KuCard key={item.ku.cid_hex} ku={item.ku} rank={i + 1}
                    badge={<ScoreBadge label={t('discovery.relevance')} value={item.relevance} />}
                    reason={item.reason} />
                ))}
              </div>
            )
          )}

          {/* ── Domains ─── */}
          {tab === 'domains' && (
            domains.length === 0 ? (
              <EmptyState icon={<Layers size={48} />} message={t('discovery.noDomains')} />
            ) : selectedDomain ? (
              <div>
                <button onClick={() => setSelectedDomain(null)}
                  style={{
                    background: 'none', border: 'none', color: 'var(--ob-accent)',
                    cursor: 'pointer', fontSize: '0.9rem', marginBottom: 16, padding: 0,
                  }}>
                  ← {t('discovery.domains')}
                </button>
                <h2 style={{ marginBottom: 16 }}>{selectedDomain} — {t('discovery.kusInDomain')}</h2>
                <div style={{ display: 'grid', gap: 12 }}>
                  {domainKus.map(ku => (
                    <KuCard key={ku.cid_hex} ku={ku} />
                  ))}
                </div>
              </div>
            ) : (
              <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 16 }}>
                {domains.map(domain => (
                  <button key={domain.name} onClick={() => loadDomainKus(domain.name)}
                    className="glass-card" style={{
                      padding: 20, border: 'none', cursor: 'pointer', textAlign: 'left',
                      background: 'var(--ob-bg-tertiary)', borderRadius: 12,
                      transition: 'all var(--ob-transition)',
                    }}>
                    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                      <div>
                        <div style={{
                          fontSize: '1rem', fontWeight: 600,
                          color: GENE_TYPE_COLORS[domain.name] || 'var(--ob-text-primary)',
                          marginBottom: 4,
                        }}>{domain.name}</div>
                        <div style={{ fontSize: '0.85rem', color: 'var(--ob-text-secondary)' }}>
                          {domain.ku_count} KUs · Avg PoMV {(domain.avg_pomv * 100).toFixed(0)}%
                        </div>
                      </div>
                      <ChevronRight size={20} style={{ color: 'var(--ob-text-tertiary)' }} />
                    </div>
                    {/* Mini bar */}
                    <div style={{ marginTop: 12, height: 4, borderRadius: 2, background: 'var(--ob-glass-border)', overflow: 'hidden' }}>
                      <div style={{
                        width: `${Math.min(domain.avg_pomv * 100, 100)}%`, height: '100%',
                        background: GENE_TYPE_COLORS[domain.name] || 'var(--ob-accent)',
                        borderRadius: 2, transition: 'width 0.5s ease',
                      }} />
                    </div>
                  </button>
                ))}
              </div>
            )
          )}
        </>
      )}
    </div>
  );
}

function KuCard({ ku, rank, badge, reason }: {
  ku: KuListItem; rank?: number;
  badge?: React.ReactNode; reason?: string;
}) {
  const color = GENE_TYPE_COLORS[ku.gene_type] || '#6366f1';
  return (
    <div className="glass-card" style={{
      padding: '16px 20px', borderRadius: 12,
      background: 'var(--ob-bg-tertiary)',
      borderLeft: `3px solid ${color}`,
      display: 'flex', alignItems: 'center', gap: 16,
      transition: 'all var(--ob-transition)',
    }}>
      {rank !== undefined && (
        <div style={{
          width: 32, height: 32, borderRadius: '50%',
          background: rank <= 3 ? 'linear-gradient(135deg, #f59e0b, #ef4444)' : 'var(--ob-bg-secondary)',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          fontWeight: 700, fontSize: '0.85rem', flexShrink: 0,
          color: rank <= 3 ? '#fff' : 'var(--ob-text-secondary)',
        }}>{rank}</div>
      )}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
          <span style={{
            padding: '2px 8px', borderRadius: 6, fontSize: '0.75rem', fontWeight: 600,
            background: `${color}20`, color,
          }}>{ku.gene_type}</span>
          <span style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)', fontFamily: 'var(--ob-font-mono)' }}>
            {ku.cid_hex.slice(0, 12)}…
          </span>
        </div>
        <div style={{
          fontSize: '0.9rem', color: 'var(--ob-text-primary)',
          overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
        }}>{ku.preview}</div>
        {reason && (
          <span style={{
            display: 'inline-block', marginTop: 4,
            padding: '2px 8px', borderRadius: 10, fontSize: '0.7rem', fontWeight: 500,
            background: `${REASON_COLORS[reason] || '#6366f1'}18`,
            color: REASON_COLORS[reason] || '#6366f1',
          }}>{REASON_LABELS[reason] || reason}</span>
        )}
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 4, flexShrink: 0 }}>
        {badge}
        <div style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)' }}>
          PoMV {(ku.pomv * 100).toFixed(0)}% · Trust {(ku.trust * 100).toFixed(0)}%
        </div>
      </div>
    </div>
  );
}

function ScoreBadge({ label, value }: { label: string; value: number }) {
  const pct = Math.round(value * 100);
  const hue = Math.round(value * 120); // 0=red, 120=green
  return (
    <div style={{
      padding: '4px 12px', borderRadius: 20, fontSize: '0.75rem', fontWeight: 600,
      background: `hsla(${hue}, 70%, 50%, 0.15)`, color: `hsl(${hue}, 70%, 50%)`,
      whiteSpace: 'nowrap',
    }}>
      {label}: {pct}%
    </div>
  );
}

function EmptyState({ icon, message }: { icon: React.ReactNode; message: string }) {
  return (
    <div style={{
      display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
      padding: '80px 20px', color: 'var(--ob-text-tertiary)', gap: 16,
    }}>
      <div style={{ opacity: 0.3 }}>{icon}</div>
      <div style={{ fontSize: '1rem' }}>{message}</div>
    </div>
  );
}
