import { useEffect, useState, useCallback, useMemo } from 'react';
import { Search, ChevronLeft, ChevronRight, X, Star, Tag, Plus, Code, ArrowUpDown, Trash2, AlertTriangle, CheckSquare, History, Bookmark, BookmarkPlus } from 'lucide-react';
import { VersionTimeline } from '../components/VersionTimeline';
import { FilePreview } from '../components/FilePreview';
import { ConsensusRing } from '../components/ConsensusRing';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import type { KuListItem, KuDetail, BlobMeta } from '../api/types';
import { ALL_GENE_TYPES } from '../api/types';
import { KqlEditor } from '../components/KqlEditor';
import { formatDate, formatSize } from '../utils/format';

const GENE_TYPES = ['All', ...ALL_GENE_TYPES];

export function ExplorerPage() {
  const { t } = useTranslation();
  const [kus, setKus] = useState<KuListItem[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [filter, setFilter] = useState('All');
  const [search, setSearch] = useState('');
  const [selectedKu, setSelectedKu] = useState<KuDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [pinnedCids, setPinnedCids] = useState<Set<string>>(new Set());
  const [showPinnedOnly, setShowPinnedOnly] = useState(false);
  const [kuTags, setKuTags] = useState<string[]>([]);
  const [newTag, setNewTag] = useState('');
  const [kqlMode, setKqlMode] = useState(false);
  const [kqlQuery, setKqlQuery] = useState('');
  const [, setKqlResults] = useState<unknown[] | null>(null);
  const [sortBy, setSortBy] = useState<'created' | 'pomv' | 'trust' | 'wire_size'>('created');
  const [sortDir, setSortDir] = useState<'asc' | 'desc'>('desc');
  const [selectedCids, setSelectedCids] = useState<Set<string>>(new Set());
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [searchHistory, setSearchHistory] = useState<Array<{ id: string; query: string; result_count: number; timestamp: number }>>([]);
  const [savedSearches, setSavedSearches] = useState<Array<{ id: string; name: string; query: string; is_kql: boolean }>>([]);
  const [saveName, setSaveName] = useState('');
  const [showSaveInput, setShowSaveInput] = useState(false);
  const [versions, setVersions] = useState<Array<{ cid_hex: string; gene_type: string; preview: string; version: number; created: number }>>([]);
  const [attachments, setAttachments] = useState<BlobMeta[]>([]);
  const [suggestions, setSuggestions] = useState<{ tags: string[]; gene_types: string[]; kus: KuListItem[] } | null>(null);
  const [showSuggest, setShowSuggest] = useState(false);
  const limit = 15;

  const loadKus = useCallback(() => {
    setLoading(true);
    const geneType = filter === 'All' ? undefined : filter;
    if (search.trim()) {
      api.search(search, limit).then((data) => {
        const typed = data as { results?: KuListItem[] };
        const results = Array.isArray(typed.results) ? typed.results : Array.isArray(data) ? (data as KuListItem[]) : [];
        setKus(results);
        setTotal(results.length);
      }).catch(() => { setKus([]); setTotal(0); }).finally(() => setLoading(false));
    } else {
      api.listKus(page, limit, geneType)
        .then(r => { setKus(r.kus); setTotal(r.total); })
        .catch(() => { setKus([]); setTotal(0); })
        .finally(() => setLoading(false));
    }
  }, [page, filter, search]);

  useEffect(() => { loadKus(); }, [loadKus]);

  // Load pinned KUs on mount
  useEffect(() => {
    api.listPinnedKus().then(pinned => {
      setPinnedCids(new Set(pinned.map(k => k.cid_hex)));
    }).catch(() => {});
  }, []);

  const togglePin = useCallback(async (cid: string, e?: React.MouseEvent) => {
    e?.stopPropagation();
    const isPinned = pinnedCids.has(cid);
    try {
      if (isPinned) {
        await api.unpinKu(cid);
        setPinnedCids(prev => { const next = new Set(prev); next.delete(cid); return next; });
      } else {
        await api.pinKu(cid);
        setPinnedCids(prev => new Set(prev).add(cid));
      }
    } catch (e) { console.error('Pin toggle failed:', e); }
  }, [pinnedCids]);

  const addTag = async () => {
    if (!newTag.trim() || !selectedKu) return;
    try {
      await api.addTag(selectedKu.cid_hex, newTag.trim());
      setKuTags(prev => [...prev, newTag.trim()]);
      setNewTag('');
    } catch (e) { console.error('Add tag failed:', e); }
  };

  const removeTag = async (tag: string) => {
    if (!selectedKu) return;
    try {
      await api.removeTag(selectedKu.cid_hex, tag);
      setKuTags(prev => prev.filter(t => t !== tag));
    } catch (e) { console.error('Remove tag failed:', e); }
  };

  const handleSearch = () => {
    setPage(1);
    if (search.trim()) {
      api.recordSearch(search.trim(), 0).catch(() => {});
    }
    loadKus();
    setShowHistory(false);
  };

  const loadHistory = () => {
    api.listSearchHistory(20).then(d => setSearchHistory(d.history || [])).catch(() => {});
    api.listSavedSearches().then(d => setSavedSearches(d.saved_searches || [])).catch(() => {});
  };

  // Debounced search suggest
  useEffect(() => {
    if (search.trim().length < 2) { setSuggestions(null); return; }
    const timer = setTimeout(() => {
      api.searchSuggest(search.trim(), 5).then(s => {
        setSuggestions(s);
        setShowSuggest(true);
      }).catch(() => setSuggestions(null));
    }, 300);
    return () => clearTimeout(timer);
  }, [search]);

  const handleSaveSearch = async () => {
    if (!saveName.trim() || !search.trim()) return;
    await api.saveSearch(saveName.trim(), search.trim(), kqlMode);
    setSaveName('');
    setShowSaveInput(false);
    loadHistory();
  };

  const applySavedSearch = (query: string, isKql: boolean) => {
    if (isKql) {
      setKqlMode(true);
      setKqlQuery(query);
    } else {
      setKqlMode(false);
      setSearch(query);
    }
    setShowHistory(false);
  };

  const handleKqlSearch = async () => {
    if (!kqlQuery.trim()) return;
    setLoading(true);
    try {
      const res = await api.kql(kqlQuery);
      setKqlResults(res.results);
    } catch { setKqlResults([]); }
    finally { setLoading(false); }
  };

  const openDetail = async (cid: string) => {
    try {
      const detail = await api.getKu(cid);
      setSelectedKu(detail);
      // Load tags for this specific KU
      api.getKuTags(cid).then(r => {
        setKuTags(r.tags || []);
      }).catch(() => setKuTags([]));
      // Load version chain
      api.getVersionChain(cid).then(r => {
        setVersions(r.versions || []);
      }).catch(() => setVersions([]));
      // Load attachments — show blobs referenced in KU's media_refs
      const mediaRefs = (detail as KuDetail & { media_refs?: { cid_hex: string }[] }).media_refs;
      if (mediaRefs && mediaRefs.length > 0) {
        api.listBlobs().then(r => {
          const mediaCids = new Set(mediaRefs.map((m: { cid_hex: string }) => m.cid_hex));
          setAttachments((r.blobs || []).filter((b: BlobMeta) => mediaCids.has(b.blob_cid_hex)));
        }).catch(() => setAttachments([]));
      } else {
        setAttachments([]);
      }
    } catch { /* ignore */ }
  };

  const totalPages = Math.max(1, Math.ceil(total / limit));
  const displayKus = useMemo(() => {
    let filtered = showPinnedOnly ? kus.filter(k => pinnedCids.has(k.cid_hex)) : kus;
    filtered = [...filtered].sort((a, b) => {
      const av = a[sortBy];
      const bv = b[sortBy];
      return sortDir === 'asc' ? (av > bv ? 1 : -1) : (av < bv ? 1 : -1);
    });
    return filtered;
  }, [kus, showPinnedOnly, pinnedCids, sortBy, sortDir]);

  const toggleSelectAll = () => {
    if (selectedCids.size === displayKus.length) {
      setSelectedCids(new Set());
    } else {
      setSelectedCids(new Set(displayKus.map(k => k.cid_hex)));
    }
  };

  const toggleSelect = (cid: string, e: React.MouseEvent) => {
    e.stopPropagation();
    setSelectedCids(prev => {
      const next = new Set(prev);
      if (next.has(cid)) next.delete(cid); else next.add(cid);
      return next;
    });
  };

  const handleBulkDelete = async () => {
    if (selectedCids.size === 0) return;
    if (!confirm(t('explorer.confirmBulkDelete', { count: selectedCids.size }))) return;
    setBulkDeleting(true);
    try {
      await Promise.allSettled(Array.from(selectedCids).map(cid => api.deleteKu(cid)));
      setSelectedCids(new Set());
      loadKus();
    } catch { /* ignore */ }
    finally { setBulkDeleting(false); }
  };

  const handleDeprecate = async (cid: string) => {
    try {
      await api.deprecateKu(cid);
      loadKus();
    } catch { /* ignore */ }
  };

  return (
    <div className="page">
      <div className="page-header">
        <h1>{t('explorer.title')}</h1>
        <p>{t('explorer.total', { count: total })}</p>
      </div>

      {/* Search + Filters */}
      <div style={{ display: 'flex', gap: 'var(--ob-gap-md)', marginBottom: 'var(--ob-gap-md)', alignItems: 'center' }}>
        <button
          className={`btn btn-sm ${kqlMode ? 'btn-primary' : ''}`}
          onClick={() => { setKqlMode(!kqlMode); setKqlResults(null); }}
          style={{ display: 'flex', alignItems: 'center', gap: 4 }}
          title="Toggle KQL Mode"
        >
          <Code size={14} /> KQL
        </button>
        {!kqlMode ? (
          <>
            <div style={{ flex: 1, position: 'relative' }}>
              <Search size={16} style={{ position: 'absolute', left: 12, top: '50%', transform: 'translateY(-50%)', color: 'var(--ob-text-muted)' }} />
              <input
                className="input"
                placeholder={t('explorer.searchPlaceholder')}
                value={search}
                onChange={e => setSearch(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleSearch()}
                onFocus={() => { loadHistory(); setShowHistory(true); }}
                onBlur={() => setTimeout(() => { setShowHistory(false); setShowSuggest(false); }, 200)}
                style={{ paddingLeft: 36, paddingRight: 60 }}
              />
              {/* Save search button */}
              {search.trim() && (
                <button onClick={() => setShowSaveInput(!showSaveInput)}
                  style={{ position: 'absolute', right: 36, top: '50%', transform: 'translateY(-50%)', background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-secondary)', padding: 2 }}
                  title={t('searchHistory.saveThis')}>
                  <BookmarkPlus size={14} />
                </button>
              )}
              {/* History button */}
              <button onClick={() => { loadHistory(); setShowHistory(!showHistory); }}
                style={{ position: 'absolute', right: 10, top: '50%', transform: 'translateY(-50%)', background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-secondary)', padding: 2 }}
                title={t('searchHistory.title')}>
                <History size={14} />
              </button>
              {/* Search History Dropdown */}
              {showHistory && (searchHistory.length > 0 || savedSearches.length > 0) && (
                <div style={{
                  position: 'absolute', top: '100%', left: 0, right: 0, marginTop: 4,
                  background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)',
                  borderRadius: 10, padding: 8, zIndex: 50, maxHeight: 300, overflowY: 'auto',
                  boxShadow: '0 8px 32px rgba(0,0,0,0.3)',
                }}>
                  {savedSearches.length > 0 && (
                    <>
                      <div style={{ fontSize: '0.72rem', fontWeight: 600, color: 'var(--ob-text-tertiary)', padding: '4px 8px', textTransform: 'uppercase' }}>
                        <Bookmark size={10} style={{ display: 'inline', marginRight: 4 }} />{t('searchHistory.savedSearches')}
                      </div>
                      {savedSearches.map(ss => (
                        <button key={ss.id}
                          onMouseDown={() => applySavedSearch(ss.query, ss.is_kql)}
                          style={{ display: 'flex', width: '100%', alignItems: 'center', gap: 8, padding: '6px 8px', borderRadius: 6, background: 'transparent', border: 'none', cursor: 'pointer', color: 'var(--ob-text-primary)', fontSize: '0.85rem', textAlign: 'left' }}>
                          <Bookmark size={12} style={{ color: 'var(--ob-accent)', flexShrink: 0 }} />
                          <span style={{ flex: 1 }}>{ss.name}</span>
                          <span style={{ fontSize: '0.7rem', color: 'var(--ob-text-tertiary)' }}>{ss.query.slice(0, 30)}</span>
                        </button>
                      ))}
                    </>
                  )}
                  {searchHistory.length > 0 && (
                    <>
                      <div style={{ fontSize: '0.72rem', fontWeight: 600, color: 'var(--ob-text-tertiary)', padding: '4px 8px', marginTop: savedSearches.length > 0 ? 8 : 0, textTransform: 'uppercase' }}>
                        <History size={10} style={{ display: 'inline', marginRight: 4 }} />{t('searchHistory.title')}
                      </div>
                      {searchHistory.map(h => (
                        <button key={h.id}
                          onMouseDown={() => { setSearch(h.query); setShowHistory(false); }}
                          style={{ display: 'flex', width: '100%', alignItems: 'center', gap: 8, padding: '6px 8px', borderRadius: 6, background: 'transparent', border: 'none', cursor: 'pointer', color: 'var(--ob-text-primary)', fontSize: '0.85rem', textAlign: 'left' }}>
                          <History size={12} style={{ color: 'var(--ob-text-tertiary)', flexShrink: 0 }} />
                          <span style={{ flex: 1 }}>{h.query}</span>
                          <span style={{ fontSize: '0.7rem', color: 'var(--ob-text-tertiary)' }}>{h.result_count} results</span>
                        </button>
                      ))}
                    </>
                  )}
                </div>
              )}
              {/* Save Search Input */}
              {showSaveInput && (
                <div style={{
                  position: 'absolute', top: '100%', left: 0, right: 0, marginTop: 4,
                  background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)',
                  borderRadius: 10, padding: 12, zIndex: 51, display: 'flex', gap: 8,
                  boxShadow: '0 8px 32px rgba(0,0,0,0.3)',
                }}>
                  <input value={saveName} onChange={e => setSaveName(e.target.value)} placeholder={t('searchHistory.saveName')}
                    onKeyDown={e => e.key === 'Enter' && handleSaveSearch()}
                    style={{ flex: 1, padding: '8px 12px', borderRadius: 6, background: 'var(--ob-bg-tertiary)', border: '1px solid var(--ob-glass-border)', color: 'var(--ob-text-primary)', fontSize: '0.85rem' }} />
                  <button onClick={handleSaveSearch} className="btn-primary" style={{ padding: '8px 16px', borderRadius: 6, fontSize: '0.85rem' }}>
                    {t('common.save')}
                  </button>
                </div>
              )}
              {/* Search Suggestions Dropdown */}
              {showSuggest && suggestions && !showHistory && (
                (suggestions.tags.length > 0 || suggestions.gene_types.length > 0 || suggestions.kus.length > 0) && (
                  <div style={{
                    position: 'absolute', top: '100%', left: 0, right: 0, marginTop: 4,
                    background: 'var(--ob-bg-secondary)', border: '1px solid var(--ob-glass-border)',
                    borderRadius: 10, padding: 8, zIndex: 52, maxHeight: 280, overflowY: 'auto',
                    boxShadow: '0 8px 32px rgba(0,0,0,0.3)',
                  }}>
                    {suggestions.tags.length > 0 && (
                      <>
                        <div style={{ fontSize: '0.7rem', fontWeight: 600, color: 'var(--ob-text-muted)', padding: '4px 8px' }}>
                          🏷️ Tags
                        </div>
                        {suggestions.tags.map(tag => (
                          <button key={tag}
                            onMouseDown={() => { setSearch(`tag:${tag}`); setShowSuggest(false); }}
                            style={{ display: 'block', width: '100%', padding: '6px 8px', borderRadius: 6, background: 'transparent', border: 'none', cursor: 'pointer', color: 'var(--ob-accent)', fontSize: '0.85rem', textAlign: 'left' }}>
                            #{tag}
                          </button>
                        ))}
                      </>
                    )}
                    {suggestions.gene_types.length > 0 && (
                      <>
                        <div style={{ fontSize: '0.7rem', fontWeight: 600, color: 'var(--ob-text-muted)', padding: '4px 8px', marginTop: 4 }}>
                          🧬 Gene Types
                        </div>
                        {suggestions.gene_types.map(gt => (
                          <button key={gt}
                            onMouseDown={() => { setFilter(gt); setShowSuggest(false); }}
                            style={{ display: 'block', width: '100%', padding: '6px 8px', borderRadius: 6, background: 'transparent', border: 'none', cursor: 'pointer', color: 'var(--ob-violet)', fontSize: '0.85rem', textAlign: 'left' }}>
                            {gt}
                          </button>
                        ))}
                      </>
                    )}
                    {suggestions.kus.length > 0 && (
                      <>
                        <div style={{ fontSize: '0.7rem', fontWeight: 600, color: 'var(--ob-text-muted)', padding: '4px 8px', marginTop: 4 }}>
                          📄 Knowledge Units
                        </div>
                        {suggestions.kus.map(ku => (
                          <button key={ku.cid_hex}
                            onMouseDown={() => { openDetail(ku.cid_hex); setShowSuggest(false); }}
                            style={{ display: 'block', width: '100%', padding: '6px 8px', borderRadius: 6, background: 'transparent', border: 'none', cursor: 'pointer', color: 'var(--ob-text-primary)', fontSize: '0.82rem', textAlign: 'left' }}>
                            <span style={{ color: 'var(--ob-text-muted)', fontFamily: 'var(--ob-font-mono)', fontSize: '0.72rem' }}>{ku.cid_hex.slice(0, 8)}…</span>{' '}
                            {ku.preview}
                          </button>
                        ))}
                      </>
                    )}
                  </div>
                )
              )}
            </div>
            <button className="btn btn-primary" onClick={handleSearch}><Search size={16} /> {t('common.search')}</button>
          </>
        ) : (
          <div style={{ flex: 1 }}>
            <KqlEditor
              value={kqlQuery}
              onChange={setKqlQuery}
              onSubmit={handleKqlSearch}
              placeholder="SELECT * FROM kus WHERE gene_type = 'Fact' ORDER BY pomv DESC LIMIT 10"
              minHeight={80}
            />
            <button className="btn btn-primary" onClick={handleKqlSearch} style={{ marginTop: 8 }}>
              <Search size={16} /> Execute KQL
            </button>
          </div>
        )}
      </div>

      {/* Filter chips */}
      <div style={{ display: 'flex', gap: 'var(--ob-gap-sm)', marginBottom: 'var(--ob-gap-lg)', flexWrap: 'wrap' }}>
        {GENE_TYPES.map(gt => (
          <button
            key={gt}
            className={`btn btn-sm ${filter === gt ? 'btn-primary' : ''}`}
            onClick={() => { setFilter(gt); setPage(1); }}
          >{gt}</button>
        ))}
        <div style={{ width: 1, height: 24, background: 'var(--ob-glass-border)', margin: '0 4px' }} />
        <button
          className={`btn btn-sm ${showPinnedOnly ? 'btn-primary' : ''}`}
          onClick={() => setShowPinnedOnly(!showPinnedOnly)}
          style={{ display: 'flex', alignItems: 'center', gap: 4 }}
        >
          <Star size={14} fill={showPinnedOnly ? 'currentColor' : 'none'} />
          {t('explorer.pinned')}
        </button>

        {/* Sort dropdown */}
        <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 6 }}>
          <ArrowUpDown size={14} style={{ color: 'var(--ob-text-muted)' }} />
          <select
            value={`${sortBy}-${sortDir}`}
            onChange={e => {
              const [field, dir] = e.target.value.split('-');
              setSortBy(field as typeof sortBy);
              setSortDir(dir as typeof sortDir);
            }}
            style={{
              background: 'var(--ob-surface)', border: '1px solid var(--ob-glass-border)',
              borderRadius: 'var(--ob-radius-sm)', color: 'var(--ob-text-secondary)',
              padding: '4px 8px', fontSize: '0.78rem', cursor: 'pointer',
            }}
          >
            <option value="created-desc">{t('explorer.sortCreated')} ↓</option>
            <option value="created-asc">{t('explorer.sortCreated')} ↑</option>
            <option value="pomv-desc">{t('explorer.sortPomv')} ↓</option>
            <option value="pomv-asc">{t('explorer.sortPomv')} ↑</option>
            <option value="trust-desc">{t('explorer.sortTrust')} ↓</option>
            <option value="wire_size-desc">Size ↓</option>
          </select>
        </div>
      </div>

      {/* Bulk Action Bar */}
      {selectedCids.size > 0 && (
        <div style={{
          display: 'flex', alignItems: 'center', gap: 'var(--ob-gap-md)',
          padding: '10px 16px', marginBottom: 'var(--ob-gap-md)',
          background: 'rgba(99, 102, 241, 0.1)', border: '1px solid rgba(99, 102, 241, 0.2)',
          borderRadius: 'var(--ob-radius-md)',
        }}>
          <CheckSquare size={16} style={{ color: '#a5b4fc' }} />
          <span style={{ fontSize: '0.85rem', color: '#a5b4fc', fontWeight: 600 }}>
            {selectedCids.size} selected
          </span>
          <div style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
            <button
              className="btn btn-sm"
              onClick={handleBulkDelete}
              disabled={bulkDeleting}
              style={{ color: '#f87171', borderColor: 'rgba(248, 113, 113, 0.3)' }}
            >
              <Trash2 size={14} /> {t('explorer.bulkDelete')}
            </button>
            <button className="btn btn-sm" onClick={() => setSelectedCids(new Set())}>
              {t('common.cancel')}
            </button>
          </div>
        </div>
      )}

      {/* Table */}
      <div className="glass-card" style={{ padding: 0, overflow: 'hidden' }}>
        {loading ? (
          <div style={{ display: 'flex', justifyContent: 'center', padding: 40 }}><div className="spinner" /></div>
        ) : kus.length === 0 ? (
          <div className="empty-state"><Search size={40} /><p>No knowledge units found</p></div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <td style={{ width: 30 }}>
                  <input
                    type="checkbox"
                    checked={selectedCids.size === displayKus.length && displayKus.length > 0}
                    onChange={toggleSelectAll}
                    style={{ cursor: 'pointer', accentColor: 'var(--ob-accent)' }}
                  />
                </td>
                <th></th><th>CID</th><th>Type</th><th>Preview</th><th>Legacy PoMV</th><th>Trust</th><th>Size</th><th>Date</th><th></th>
              </tr>
            </thead>
            <tbody>
              {displayKus.map(ku => (
                <tr key={ku.cid_hex} onClick={() => openDetail(ku.cid_hex)} style={{ cursor: 'pointer', background: selectedCids.has(ku.cid_hex) ? 'rgba(99, 102, 241, 0.08)' : undefined }}>
                  <td>
                    <input
                      type="checkbox"
                      checked={selectedCids.has(ku.cid_hex)}
                      onClick={(e) => toggleSelect(ku.cid_hex, e)}
                      onChange={() => {}}
                      style={{ cursor: 'pointer', accentColor: 'var(--ob-accent)' }}
                    />
                  </td>
                  <td>
                    <button
                      onClick={(e) => togglePin(ku.cid_hex, e)}
                      style={{ background: 'none', border: 'none', cursor: 'pointer', padding: 2, color: pinnedCids.has(ku.cid_hex) ? '#f59e0b' : 'var(--ob-text-muted)', transition: 'color 0.2s' }}
                    >
                      <Star size={16} fill={pinnedCids.has(ku.cid_hex) ? '#f59e0b' : 'none'} />
                    </button>
                  </td>
                  <td className="mono">{ku.cid_hex.slice(0, 10)}…</td>
                  <td><span className="badge badge-cyan">{ku.gene_type}</span></td>
                  <td style={{ maxWidth: 280, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{ku.preview}</td>
                  <td>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <ConsensusRing pomv={ku.pomv} size={32} showLabel={true} />
                    </div>
                  </td>
                  <td style={{ fontSize: '0.82rem' }}>{(ku.trust * 100).toFixed(0)}%</td>
                  <td style={{ fontSize: '0.82rem', color: 'var(--ob-text-tertiary)' }}>{formatSize(ku.wire_size)}</td>
                  <td style={{ fontSize: '0.82rem', color: 'var(--ob-text-tertiary)' }}>{formatDate(ku.created)}</td>
                  <td>
                    <button
                      onClick={(e) => { e.stopPropagation(); handleDeprecate(ku.cid_hex); }}
                      title={t('explorer.deprecate')}
                      style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--ob-text-muted)', padding: 2, transition: 'color 0.2s' }}
                    >
                      <AlertTriangle size={14} />
                    </button>
                  </td>
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
              <button
                onClick={() => togglePin(selectedKu.cid_hex)}
                style={{
                  background: 'none', border: '1px solid var(--ob-glass-border)',
                  borderRadius: 'var(--ob-radius-sm)', cursor: 'pointer', padding: '4px 10px',
                  color: pinnedCids.has(selectedKu.cid_hex) ? '#f59e0b' : 'var(--ob-text-muted)',
                  display: 'flex', alignItems: 'center', gap: 4, fontSize: '0.82rem',
                  transition: 'all 0.2s',
                }}
              >
                <Star size={14} fill={pinnedCids.has(selectedKu.cid_hex) ? '#f59e0b' : 'none'} />
                {pinnedCids.has(selectedKu.cid_hex) ? t('explorer.unpin') : t('explorer.pin')}
              </button>
            </div>
            {/* Tags Section */}
            <div>
              <span className="stat-label" style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <Tag size={14} />
                {t('explorer.tags')}
              </span>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 6 }}>
                {kuTags.map(tag => (
                  <span key={tag} style={{
                    display: 'inline-flex', alignItems: 'center', gap: 4,
                    padding: '3px 10px', borderRadius: 12,
                    background: 'rgba(99, 102, 241, 0.2)', border: '1px solid rgba(99, 102, 241, 0.3)',
                    color: '#a5b4fc', fontSize: '0.78rem',
                  }}>
                    #{tag}
                    <button onClick={() => removeTag(tag)} style={{
                      background: 'none', border: 'none', cursor: 'pointer',
                      color: 'var(--ob-text-muted)', padding: 0, display: 'flex',
                    }}>
                      <X size={12} />
                    </button>
                  </span>
                ))}
                <div style={{ display: 'flex', gap: 4 }}>
                  <input
                    className="input"
                    placeholder={t('explorer.addTag')}
                    value={newTag}
                    onChange={e => setNewTag(e.target.value)}
                    onKeyDown={e => e.key === 'Enter' && addTag()}
                    style={{ width: 100, padding: '3px 8px', fontSize: '0.78rem', height: 26 }}
                  />
                  <button onClick={addTag} style={{
                    background: 'none', border: '1px solid var(--ob-glass-border)',
                    borderRadius: 'var(--ob-radius-sm)', cursor: 'pointer',
                    color: 'var(--ob-text-secondary)', display: 'flex', alignItems: 'center', padding: '0 6px',
                  }}>
                    <Plus size={14} />
                  </button>
                </div>
              </div>
            </div>
            <div>
              <span className="stat-label">Content</span>
              <p style={{ fontSize: '0.88rem', lineHeight: 1.6, marginTop: 4 }}>{selectedKu.content}</p>
            </div>
            <div className="grid-2">
              <div><span className="stat-label">Legacy PoMV (non-economic)</span><p style={{ fontSize: '1.1rem', fontWeight: 600 }}>{(selectedKu.pomv * 100).toFixed(1)}%</p></div>
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
            {selectedKu.decoded_instructions && selectedKu.decoded_instructions.length > 0 && (
              <div>
                <span className="stat-label">Decoded Instructions ({selectedKu.decoded_instructions.length})</span>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 6, marginTop: 8 }}>
                  {selectedKu.decoded_instructions.map((instr, i) => (
                    <div key={i} style={{
                      padding: '8px 12px',
                      borderRadius: 'var(--ob-radius-sm)',
                      background: 'var(--ob-surface)',
                      border: '1px solid var(--ob-glass-border)',
                      fontSize: '0.82rem',
                    }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                        <span style={{
                          background: '#00e5ff',
                          color: '#000',
                          padding: '2px 10px',
                          borderRadius: 10,
                          fontSize: '0.72rem',
                          fontWeight: 700,
                          textTransform: 'uppercase',
                          letterSpacing: '0.05em',
                        }}>{instr.op}</span>
                        <span className="mono" style={{ fontSize: '0.72rem', color: 'var(--ob-text-muted)' }}>
                          IDs: [{instr.concept_ids.join(', ')}]
                        </span>
                      </div>
                      <div style={{ color: '#e0f7fa', fontFamily: 'var(--ob-font-mono, monospace)', fontSize: '0.85rem' }}>
                        {instr.description}
                      </div>
                    </div>
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
            {/* Attachments */}
            {attachments.length > 0 && (
              <div>
                <span className="stat-label" style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8 }}>
                  📎 Attachments ({attachments.length})
                </span>
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                  {attachments.map(blob => (
                    <FilePreview key={blob.blob_cid_hex} blob={blob} />
                  ))}
                </div>
              </div>
            )}
            {/* Version History */}
            <VersionTimeline
              versions={versions}
              currentCid={selectedKu.cid_hex}
              onNavigate={(cid) => openDetail(cid)}
            />
          </div>
        </div>
      )}
    </div>
  );
}
