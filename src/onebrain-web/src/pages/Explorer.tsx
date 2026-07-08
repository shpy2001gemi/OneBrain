import { useEffect, useState } from 'react';
import { Search, ChevronLeft, ChevronRight, X } from 'lucide-react';
import { api } from '../api/client';
import type { KuListItem, KuDetail } from '../api/types';

const GENE_TYPES = ['All', 'Fact', 'Procedure', 'Experience', 'Creative', 'MediaExperience', 'Testimony', 'Formal', 'Hypothesis', 'Narrative', 'Sensory', 'Composite'];

export function ExplorerPage() {
  const [kus, setKus] = useState<KuListItem[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [filter, setFilter] = useState('All');
  const [search, setSearch] = useState('');
  const [selectedKu, setSelectedKu] = useState<KuDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const limit = 15;

  const loadKus = () => {
    setLoading(true);
    const geneType = filter === 'All' ? undefined : filter;
    if (search.trim()) {
      api.search(search, limit).then((data: any) => {
        const results = Array.isArray(data) ? data : [];
        setKus(results);
        setTotal(results.length);
      }).catch(() => { setKus([]); setTotal(0); }).finally(() => setLoading(false));
    } else {
      api.listKus(page, limit, geneType)
        .then(r => { setKus(r.kus); setTotal(r.total); })
        .catch(() => { setKus([]); setTotal(0); })
        .finally(() => setLoading(false));
    }
  };

  useEffect(() => { loadKus(); }, [page, filter]);

  const handleSearch = () => { setPage(1); loadKus(); };

  const openDetail = async (cid: string) => {
    try {
      const detail = await api.getKu(cid);
      setSelectedKu(detail);
    } catch { /* ignore */ }
  };

  const totalPages = Math.max(1, Math.ceil(total / limit));
  const formatDate = (ts: number) => ts ? new Date(ts * 1000).toLocaleDateString() : '—';
  const formatSize = (b: number) => b > 1024 ? `${(b / 1024).toFixed(1)}KB` : `${b}B`;

  return (
    <div className="page">
      <div className="page-header">
        <h1>Knowledge Explorer</h1>
        <p>{total} knowledge units in your brain</p>
      </div>

      {/* Search + Filters */}
      <div style={{ display: 'flex', gap: 'var(--ob-gap-md)', marginBottom: 'var(--ob-gap-md)', alignItems: 'center' }}>
        <div style={{ flex: 1, position: 'relative' }}>
          <Search size={16} style={{ position: 'absolute', left: 12, top: '50%', transform: 'translateY(-50%)', color: 'var(--ob-text-muted)' }} />
          <input
            className="input"
            placeholder="Search knowledge..."
            value={search}
            onChange={e => setSearch(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleSearch()}
            style={{ paddingLeft: 36 }}
          />
        </div>
        <button className="btn btn-primary" onClick={handleSearch}><Search size={16} /> Search</button>
      </div>

      {/* Filter chips */}
      <div style={{ display: 'flex', gap: 'var(--ob-gap-sm)', marginBottom: 'var(--ob-gap-lg)', flexWrap: 'wrap' }}>
        {GENE_TYPES.map(t => (
          <button
            key={t}
            className={`btn btn-sm ${filter === t ? 'btn-primary' : ''}`}
            onClick={() => { setFilter(t); setPage(1); }}
          >{t}</button>
        ))}
      </div>

      {/* Table */}
      <div className="glass-card" style={{ padding: 0, overflow: 'hidden' }}>
        {loading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: 40 }}><div className="spinner" /></div>
        ) : kus.length === 0 ? (
          <div className="empty-state"><Search size={40} /><p>No knowledge units found</p></div>
        ) : (
          <table className="data-table">
            <thead><tr><th>CID</th><th>Type</th><th>Preview</th><th>PoMV</th><th>Trust</th><th>Size</th><th>Date</th></tr></thead>
            <tbody>
              {kus.map(ku => (
                <tr key={ku.cid_hex} onClick={() => openDetail(ku.cid_hex)} style={{ cursor: 'pointer' }}>
                  <td className="mono">{ku.cid_hex.slice(0, 10)}…</td>
                  <td><span className="badge badge-cyan">{ku.gene_type}</span></td>
                  <td style={{ maxWidth: 280, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{ku.preview}</td>
                  <td>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <div className="progress-bar" style={{ width: 50 }}><div className="fill" style={{ width: `${ku.pomv * 100}%` }} /></div>
                      <span style={{ fontSize: '0.78rem' }}>{(ku.pomv * 100).toFixed(0)}%</span>
                    </div>
                  </td>
                  <td style={{ fontSize: '0.82rem' }}>{(ku.trust * 100).toFixed(0)}%</td>
                  <td style={{ fontSize: '0.82rem', color: 'var(--ob-text-tertiary)' }}>{formatSize(ku.wire_size)}</td>
                  <td style={{ fontSize: '0.82rem', color: 'var(--ob-text-tertiary)' }}>{formatDate(ku.created)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div style={{ display: 'flex', justifyContent: 'center', gap: 'var(--ob-gap-sm)', marginTop: 'var(--ob-gap-md)' }}>
          <button className="btn btn-sm" disabled={page <= 1} onClick={() => setPage(p => p - 1)}><ChevronLeft size={14} /></button>
          <span style={{ padding: '4px 12px', fontSize: '0.85rem', color: 'var(--ob-text-secondary)' }}>{page} / {totalPages}</span>
          <button className="btn btn-sm" disabled={page >= totalPages} onClick={() => setPage(p => p + 1)}><ChevronRight size={14} /></button>
        </div>
      )}

      {/* Detail Panel */}
      {selectedKu && (
        <div style={{
          position: 'fixed', top: 0, right: 0, bottom: 0, width: 480,
          background: 'var(--ob-bg-secondary)', borderLeft: '1px solid var(--ob-glass-border)',
          zIndex: 100, overflow: 'auto', padding: 'var(--ob-gap-lg)',
          animation: 'slideInLeft 0.3s ease both',
        }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 'var(--ob-gap-lg)' }}>
            <h3 style={{ fontSize: '1.1rem', fontWeight: 600 }}>KU Detail</h3>
            <button className="btn btn-icon" onClick={() => setSelectedKu(null)}><X size={18} /></button>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--ob-gap-md)' }}>
            <div>
              <span className="stat-label">CID</span>
              <p className="mono" style={{ fontSize: '0.78rem', wordBreak: 'break-all' }}>{selectedKu.cid_hex}</p>
            </div>
            <div style={{ display: 'flex', gap: 'var(--ob-gap-sm)' }}>
              <span className="badge badge-cyan">{selectedKu.gene_type}</span>
              <span className="badge badge-violet">{selectedKu.epistemic}</span>
              <span className="badge badge-green">{selectedKu.verification_status}</span>
            </div>
            <div>
              <span className="stat-label">Content</span>
              <p style={{ fontSize: '0.88rem', lineHeight: 1.6, marginTop: 4 }}>{selectedKu.content}</p>
            </div>
            <div className="grid-2">
              <div><span className="stat-label">PoMV</span><p style={{ fontSize: '1.1rem', fontWeight: 600 }}>{(selectedKu.pomv * 100).toFixed(1)}%</p></div>
              <div><span className="stat-label">Trust</span><p style={{ fontSize: '1.1rem', fontWeight: 600 }}>{(selectedKu.trust * 100).toFixed(1)}%</p></div>
              <div><span className="stat-label">Confidence</span><p style={{ fontSize: '1.1rem', fontWeight: 600 }}>{(selectedKu.confidence * 100).toFixed(1)}%</p></div>
              <div><span className="stat-label">Wire Size</span><p style={{ fontSize: '1.1rem', fontWeight: 600 }}>{selectedKu.wire_size}B</p></div>
            </div>
            {selectedKu.codons.length > 0 && (
              <div>
                <span className="stat-label">Codons ({selectedKu.codons.length})</span>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, marginTop: 4 }}>
                  {selectedKu.codons.map((c, i) => (
                    <span key={i} className="badge badge-violet">{c.name}</span>
                  ))}
                </div>
              </div>
            )}
            {selectedKu.bonds.length > 0 && (
              <div>
                <span className="stat-label">Bonds ({selectedKu.bonds.length})</span>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 4, marginTop: 4 }}>
                  {selectedKu.bonds.slice(0, 10).map((b, i) => (
                    <div key={i} style={{ display: 'flex', gap: 8, fontSize: '0.82rem', padding: '4px 8px', borderRadius: 'var(--ob-radius-sm)', background: 'var(--ob-surface)' }}>
                      <span className={`badge ${b.direction === 'OUT' ? 'badge-cyan' : 'badge-amber'}`}>{b.direction}</span>
                      <span style={{ color: 'var(--ob-text-secondary)' }}>{b.relation}</span>
                      <span className="mono" style={{ marginLeft: 'auto' }}>{b.other_cid.slice(0, 8)}…</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
