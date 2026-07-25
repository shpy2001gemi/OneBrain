import { useEffect, useState, useCallback } from 'react';
import { FolderOpen, Plus, Trash2, X, Package } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import type { KuListItem } from '../api/types';
import { GENE_TYPE_COLORS } from '../api/types';

type CollectionView = {
  id: string; name: string; description: string;
  ku_cids: string[]; created_at: number; updated_at: number;
};

export function CollectionsPage() {
  const { t } = useTranslation();
  const [collections, setCollections] = useState<CollectionView[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState('');
  const [newDesc, setNewDesc] = useState('');
  const [selected, setSelected] = useState<CollectionView | null>(null);
  const [collKus, setCollKus] = useState<KuListItem[]>([]);

  const load = useCallback(() => {
    setLoading(true);
    api.listCollections()
      .then(d => setCollections(d.collections || []))
      .catch(() => setCollections([]))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleCreate = async () => {
    if (!newName.trim()) return;
    await api.createCollection(newName.trim(), newDesc.trim());
    setNewName(''); setNewDesc(''); setShowCreate(false);
    load();
  };

  const handleDelete = async (id: string) => {
    if (!confirm('Delete this collection? KUs inside will not be deleted.')) return;
    await api.deleteCollection(id);
    if (selected?.id === id) { setSelected(null); setCollKus([]); }
    load();
  };

  const [, setLoadingKus] = useState(false);

  const openCollection = async (coll: CollectionView) => {
    setSelected(coll);
    setLoadingKus(true);
    try {
      // Parallel fetch — eliminates N+1 sequential requests
      const results = await Promise.allSettled(
        coll.ku_cids.slice(0, 50).map(cid => api.getKu(cid))
      );
      const kus: KuListItem[] = results
        .filter((r): r is PromiseFulfilledResult<any> => r.status === 'fulfilled')
        .map(r => ({
          cid_hex: r.value.cid_hex, gene_type: r.value.gene_type,
          preview: r.value.content.slice(0, 80), pomv: r.value.pomv,
          pomv_profile: r.value.pomv_profile, pomv_is_economic: r.value.pomv_is_economic,
          trust: r.value.trust, created: r.value.created, wire_size: r.value.wire_size,
        }));
      setCollKus(kus);
    } finally {
      setLoadingKus(false);
    }
  };

  const handleRemoveKu = async (cid: string) => {
    if (!selected) return;
    await api.removeFromCollection(selected.id, cid);
    setCollKus(prev => prev.filter(k => k.cid_hex !== cid));
    load();
  };

  const formatDate = (ts: number) => new Date(ts * 1000).toLocaleDateString();

  return (
    <div className="page">
      <div className="page-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <h1><FolderOpen size={28} style={{ display: 'inline', marginRight: 8, verticalAlign: 'middle' }} />{t('collections.title')}</h1>
        </div>
        <button onClick={() => setShowCreate(true)} className="btn-primary"
          style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '10px 20px', borderRadius: 10 }}>
          <Plus size={16} />{t('collections.create')}
        </button>
      </div>

      {/* Create Dialog */}
      {showCreate && (
        <div className="glass-card" style={{ padding: 20, marginBottom: 20, borderRadius: 12, background: 'var(--ob-bg-tertiary)' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 12 }}>
            <h3 style={{ margin: 0 }}>{t('collections.create')}</h3>
            <button onClick={() => setShowCreate(false)} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-secondary)' }}>
              <X size={18} />
            </button>
          </div>
          <input value={newName} onChange={e => setNewName(e.target.value)}
            placeholder={t('collections.name')} className="input"
            style={{ width: '100%', marginBottom: 8, padding: '10px 14px', borderRadius: 8, background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)', color: 'var(--ob-text-primary)', fontSize: '0.9rem' }}
            onKeyDown={e => e.key === 'Enter' && handleCreate()} />
          <input value={newDesc} onChange={e => setNewDesc(e.target.value)}
            placeholder={t('collections.description')} className="input"
            style={{ width: '100%', marginBottom: 12, padding: '10px 14px', borderRadius: 8, background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)', color: 'var(--ob-text-primary)', fontSize: '0.9rem' }} />
          <button onClick={handleCreate} className="btn-primary"
            style={{ padding: '8px 20px', borderRadius: 8 }}>
            {t('collections.create')}
          </button>
        </div>
      )}

      {loading ? (
        <div style={{ display: 'flex', justifyContent: 'center', paddingTop: 60 }}><div className="spinner spinner-lg" /></div>
      ) : selected ? (
        /* Collection Detail View */
        <div>
          <button onClick={() => { setSelected(null); setCollKus([]); }}
            style={{ background: 'none', border: 'none', color: 'var(--ob-accent)', cursor: 'pointer', fontSize: '0.9rem', marginBottom: 16, padding: 0 }}>
            ← {t('collections.title')}
          </button>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
            <div>
              <h2 style={{ margin: 0 }}>{selected.name}</h2>
              {selected.description && <p style={{ color: 'var(--ob-text-secondary)', margin: '4px 0 0' }}>{selected.description}</p>}
            </div>
            <span style={{ color: 'var(--ob-text-tertiary)', fontSize: '0.85rem' }}>
              {t('collections.items', { count: selected.ku_cids.length })}
            </span>
          </div>
          {collKus.length === 0 ? (
            <div style={{ textAlign: 'center', padding: 40, color: 'var(--ob-text-tertiary)' }}>
              <Package size={40} style={{ opacity: 0.3, marginBottom: 8 }} />
              <div>{t('collections.emptyDesc')}</div>
            </div>
          ) : (
            <div style={{ display: 'grid', gap: 10 }}>
              {collKus.map(ku => {
                const color = GENE_TYPE_COLORS[ku.gene_type] || '#6366f1';
                return (
                  <div key={ku.cid_hex} className="glass-card" style={{
                    padding: '14px 18px', borderRadius: 10, background: 'var(--ob-bg-tertiary)',
                    borderLeft: `3px solid ${color}`, display: 'flex', alignItems: 'center', gap: 14,
                  }}>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4 }}>
                        <span style={{ padding: '2px 6px', borderRadius: 4, fontSize: '0.7rem', fontWeight: 600, background: `${color}20`, color }}>{ku.gene_type}</span>
                        <span style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)', fontFamily: 'var(--ob-font-mono)' }}>{ku.cid_hex.slice(0, 10)}…</span>
                      </div>
                      <div style={{ fontSize: '0.9rem', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', color: 'var(--ob-text-primary)' }}>{ku.preview}</div>
                    </div>
                    <button onClick={() => handleRemoveKu(ku.cid_hex)}
                      style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-tertiary)', padding: 4 }}
                      title={t('collections.removeKu')}>
                      <X size={16} />
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ) : collections.length === 0 ? (
        <div style={{
          display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
          padding: '80px 20px', color: 'var(--ob-text-tertiary)', gap: 16,
        }}>
          <FolderOpen size={48} style={{ opacity: 0.3 }} />
          <div>{t('collections.empty')}</div>
          <div style={{ fontSize: '0.85rem' }}>{t('collections.emptyDesc')}</div>
        </div>
      ) : (
        /* Collection Grid */
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))', gap: 16 }}>
          {collections.map(coll => (
            <div key={coll.id} className="glass-card" style={{
              padding: 20, borderRadius: 12, background: 'var(--ob-bg-tertiary)',
              cursor: 'pointer', transition: 'all var(--ob-transition)',
              position: 'relative',
            }} onClick={() => openCollection(coll)}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                <div>
                  <div style={{ fontSize: '1rem', fontWeight: 600, color: 'var(--ob-text-primary)', marginBottom: 4 }}>
                    <FolderOpen size={16} style={{ display: 'inline', marginRight: 6, verticalAlign: 'middle', opacity: 0.6 }} />
                    {coll.name}
                  </div>
                  {coll.description && (
                    <div style={{ fontSize: '0.85rem', color: 'var(--ob-text-secondary)', marginBottom: 8 }}>{coll.description}</div>
                  )}
                </div>
                <button onClick={e => { e.stopPropagation(); handleDelete(coll.id); }}
                  style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-tertiary)', padding: 4 }}>
                  <Trash2 size={14} />
                </button>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: 8 }}>
                <span style={{
                  padding: '3px 10px', borderRadius: 8, fontSize: '0.75rem', fontWeight: 600,
                  background: 'var(--ob-accent-dim)', color: 'var(--ob-accent)',
                }}>
                  {t('collections.items', { count: coll.ku_cids.length })}
                </span>
                <span style={{ fontSize: '0.75rem', color: 'var(--ob-text-tertiary)' }}>
                  {formatDate(coll.updated_at)}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
