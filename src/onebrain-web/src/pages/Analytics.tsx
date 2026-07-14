import { useEffect, useState } from 'react';
import { BarChart3, TrendingUp, Database, Zap, Link2, Clock } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import { GENE_TYPE_COLORS } from '../api/types';
import { PieChart, Pie, Cell, BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts';

type Analytics = {
  total_kus: number; kus_by_type: [string, number][];
  avg_pomv: number; avg_trust: number;
  total_wire_size: number; total_bonds: number;
  kus_last_24h: number; kus_last_7d: number;
  top_gene_type: string;
};

const PIE_COLORS = ['#6366f1', '#8b5cf6', '#ec4899', '#f43f5e', '#f59e0b', '#10b981', '#06b6d4', '#3b82f6'];

export function AnalyticsPage() {
  const { t } = useTranslation();
  const [data, setData] = useState<Analytics | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.getAnalytics()
      .then(d => setData(d))
      .catch(() => setData(null))
      .finally(() => setLoading(false));
  }, []);

  const formatBytes = (bytes: number) => {
    if (bytes >= 1048576) return `${(bytes / 1048576).toFixed(1)} MB`;
    if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${bytes} B`;
  };

  if (loading) {
    return <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}><div className="spinner spinner-lg" /></div>;
  }

  if (!data) {
    return (
      <div className="page" style={{ textAlign: 'center', padding: '80px 20px', color: 'var(--ob-text-tertiary)' }}>
        <BarChart3 size={48} style={{ opacity: 0.3, marginBottom: 12 }} />
        <div>{t('analytics.noData')}</div>
      </div>
    );
  }

  const pieData = data.kus_by_type.map(([name, value]) => ({ name, value }));
  const barData = data.kus_by_type.map(([name, value]) => ({
    name, value,
    fill: GENE_TYPE_COLORS[name] || '#6366f1',
  }));

  const stats = [
    { icon: Database, label: t('analytics.totalKus'), value: data.total_kus.toLocaleString(), color: '#6366f1' },
    { icon: TrendingUp, label: t('analytics.avgPomv'), value: `${(data.avg_pomv * 100).toFixed(1)}%`, color: '#10b981' },
    { icon: Zap, label: t('analytics.avgTrust'), value: `${(data.avg_trust * 100).toFixed(1)}%`, color: '#f59e0b' },
    { icon: Link2, label: t('analytics.totalBonds'), value: data.total_bonds.toLocaleString(), color: '#ec4899' },
    { icon: Clock, label: t('analytics.last24h'), value: data.kus_last_24h.toLocaleString(), color: '#3b82f6' },
    { icon: Clock, label: t('analytics.last7d'), value: data.kus_last_7d.toLocaleString(), color: '#8b5cf6' },
  ];

  return (
    <div className="page">
      <div className="page-header">
        <h1><BarChart3 size={28} style={{ display: 'inline', marginRight: 8, verticalAlign: 'middle' }} />{t('analytics.title')}</h1>
      </div>

      {/* ── Stat Cards ─── */}
      <div style={{
        display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
        gap: 14, marginBottom: 28,
      }}>
        {stats.map(({ icon: Icon, label, value, color }) => (
          <div key={label} className="glass-card" style={{
            padding: '18px 20px', borderRadius: 12,
            background: 'var(--ob-bg-tertiary)',
            borderTop: `3px solid ${color}`,
          }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
              <Icon size={16} style={{ color, opacity: 0.8 }} />
              <span style={{ fontSize: '0.8rem', color: 'var(--ob-text-secondary)' }}>{label}</span>
            </div>
            <div style={{ fontSize: '1.5rem', fontWeight: 700, color: 'var(--ob-text-primary)' }}>{value}</div>
          </div>
        ))}
      </div>

      {/* ── Additional Info ─── */}
      <div style={{
        display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
        gap: 14, marginBottom: 28,
      }}>
        <div className="glass-card" style={{ padding: '16px 20px', borderRadius: 12, background: 'var(--ob-bg-tertiary)' }}>
          <div style={{ fontSize: '0.8rem', color: 'var(--ob-text-secondary)', marginBottom: 4 }}>{t('analytics.totalSize')}</div>
          <div style={{ fontSize: '1.3rem', fontWeight: 600, color: 'var(--ob-text-primary)' }}>{formatBytes(data.total_wire_size)}</div>
        </div>
        <div className="glass-card" style={{ padding: '16px 20px', borderRadius: 12, background: 'var(--ob-bg-tertiary)' }}>
          <div style={{ fontSize: '0.8rem', color: 'var(--ob-text-secondary)', marginBottom: 4 }}>{t('analytics.topType')}</div>
          <div style={{
            fontSize: '1.3rem', fontWeight: 600,
            color: GENE_TYPE_COLORS[data.top_gene_type] || 'var(--ob-text-primary)',
          }}>{data.top_gene_type}</div>
        </div>
      </div>

      {/* ── Charts ─── */}
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 20 }}>
        {/* Pie */}
        <div className="glass-card" style={{ padding: 20, borderRadius: 12, background: 'var(--ob-bg-tertiary)' }}>
          <h3 style={{ margin: '0 0 16px', fontSize: '1rem', fontWeight: 600 }}>{t('analytics.kusByType')}</h3>
          <ResponsiveContainer width="100%" height={280}>
            <PieChart>
              <Pie data={pieData} cx="50%" cy="50%" innerRadius={60} outerRadius={100}
                paddingAngle={2} dataKey="value" label={({ name, percent }) => `${name} ${(percent * 100).toFixed(0)}%`}
                labelLine={false} fontSize={11}>
                {pieData.map((entry, i) => (
                  <Cell key={entry.name} fill={GENE_TYPE_COLORS[entry.name] || PIE_COLORS[i % PIE_COLORS.length]} />
                ))}
              </Pie>
              <Tooltip contentStyle={{ background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)', borderRadius: 8, fontSize: '0.85rem' }} />
            </PieChart>
          </ResponsiveContainer>
        </div>

        {/* Bar */}
        <div className="glass-card" style={{ padding: 20, borderRadius: 12, background: 'var(--ob-bg-tertiary)' }}>
          <h3 style={{ margin: '0 0 16px', fontSize: '1rem', fontWeight: 600 }}>{t('analytics.activity')}</h3>
          <ResponsiveContainer width="100%" height={280}>
            <BarChart data={barData}>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--ob-glass-border)" />
              <XAxis dataKey="name" tick={{ fontSize: 11, fill: 'var(--ob-text-secondary)' }} />
              <YAxis tick={{ fontSize: 11, fill: 'var(--ob-text-secondary)' }} />
              <Tooltip contentStyle={{ background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)', borderRadius: 8, fontSize: '0.85rem' }} />
              <Bar dataKey="value" radius={[6, 6, 0, 0]}>
                {barData.map((entry) => (
                  <Cell key={entry.name} fill={entry.fill} />
                ))}
              </Bar>
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>
    </div>
  );
}
