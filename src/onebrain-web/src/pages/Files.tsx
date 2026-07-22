import { useState, useEffect, useRef } from 'react';
import { HardDrive, Upload, Download, Trash2, Search, Eye, Pin, PinOff, Paperclip } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import type { BlobMeta } from '../api/types';
import { formatSize, getMimeIcon } from '../utils/format';



export function FilesPage() {
  useTranslation();
  const [blobs, setBlobs] = useState<BlobMeta[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState('');
  const [uploading, setUploading] = useState(false);
  const [previewBlob, setPreviewBlob] = useState<BlobMeta | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [pinnedCids, setPinnedCids] = useState<Set<string>>(new Set());

  const loadBlobs = () => {
    api.listBlobs()
      .then(r => setBlobs(r.blobs || []))
      .catch(() => setBlobs([]))
      .finally(() => setLoading(false));
  };

  useEffect(() => { loadBlobs(); }, []);

  const handleUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (!files?.length) return;
    setUploading(true);
    try {
      await api.uploadBlob(files[0]);
      loadBlobs();
    } catch { /* ignore */ }
    finally {
      setUploading(false);
      if (fileInputRef.current) fileInputRef.current.value = '';
    }
  };

  const handleDelete = async (cid: string) => {
    if (!confirm('Delete this file permanently?')) return;
    try {
      await api.deleteBlob(cid);
      loadBlobs();
    } catch { /* ignore */ }
  };

  const handlePin = async (cid: string) => {
    try {
      await api.pinBlob(cid);
      setPinnedCids(prev => new Set(prev).add(cid));
    } catch { /* ignore */ }
  };

  const handleUnpin = async (cid: string) => {
    try {
      await api.unpinBlob(cid);
      setPinnedCids(prev => { const s = new Set(prev); s.delete(cid); return s; });
    } catch { /* ignore */ }
  };

  const filteredBlobs = blobs.filter(b =>
    !filter || b.original_name.toLowerCase().includes(filter.toLowerCase()) ||
    b.mime_type.toLowerCase().includes(filter.toLowerCase())
  );

  const totalSize = blobs.reduce((s, b) => s + b.total_size, 0);

  if (loading) {
    return <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}><div className="spinner spinner-lg" /></div>;
  }

  return (
    <div className="page">
      <div className="page-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <h1><HardDrive size={28} style={{ display: 'inline', marginRight: 8, verticalAlign: 'middle' }} />File Manager</h1>
          <p style={{ color: 'var(--ob-text-secondary)' }}>
            {blobs.length} files · {formatSize(totalSize)} total
          </p>
        </div>
        <div style={{ display: 'flex', gap: 8 }}>
          <input ref={fileInputRef} type="file" style={{ display: 'none' }} onChange={handleUpload} />
          <button className="btn btn-primary" onClick={() => fileInputRef.current?.click()} disabled={uploading}>
            {uploading ? <><div className="spinner" style={{ width: 14, height: 14 }} /> Uploading...</> : <><Upload size={16} /> Upload</>}
          </button>
        </div>
      </div>

      {/* Search */}
      <div style={{ position: 'relative', marginBottom: 20 }}>
        <Search size={16} style={{ position: 'absolute', left: 12, top: '50%', transform: 'translateY(-50%)', color: 'var(--ob-text-muted)' }} />
        <input
          className="input"
          placeholder="Filter files by name or type..."
          value={filter}
          onChange={e => setFilter(e.target.value)}
          style={{ paddingLeft: 36 }}
        />
      </div>

      {/* File Grid */}
      {filteredBlobs.length === 0 ? (
        <div className="glass-card" style={{ textAlign: 'center', padding: '60px 20px', color: 'var(--ob-text-tertiary)' }}>
          <HardDrive size={48} style={{ opacity: 0.3, marginBottom: 12 }} />
          <div>{filter ? 'No files matching filter' : 'No files uploaded yet'}</div>
        </div>
      ) : (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 16 }}>
          {filteredBlobs.map(blob => {
            const Icon = getMimeIcon(blob.mime_type);
            const downloadUrl = `/api/blobs/${blob.blob_cid_hex}/download`;
            const isPinned = pinnedCids.has(blob.blob_cid_hex);
            return (
              <div key={blob.blob_cid_hex} className="glass-card" style={{
                padding: 0, overflow: 'hidden', borderRadius: 12,
                transition: 'transform 0.2s, box-shadow 0.2s',
              }}>
                {/* Preview area */}
                {blob.mime_type.startsWith('image/') ? (
                  <div
                    style={{ height: 140, overflow: 'hidden', cursor: 'pointer', background: 'rgba(0,0,0,0.3)', display: 'flex', justifyContent: 'center', alignItems: 'center' }}
                    onClick={() => setPreviewBlob(blob)}
                  >
                    <img src={downloadUrl} alt={blob.original_name} style={{ maxWidth: '100%', maxHeight: 140, objectFit: 'contain' }} loading="lazy" />
                  </div>
                ) : (
                  <div
                    style={{ height: 80, display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'var(--ob-surface)', cursor: 'pointer' }}
                    onClick={() => setPreviewBlob(blob)}
                  >
                    <Icon size={36} style={{ color: 'var(--ob-accent)', opacity: 0.5 }} />
                  </div>
                )}
                {/* Info */}
                <div style={{ padding: '12px 14px' }}>
                  <div style={{
                    fontWeight: 600, fontSize: '0.88rem', marginBottom: 4,
                    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
                  }}>
                    {blob.original_name}
                  </div>
                  <div style={{ fontSize: '0.75rem', color: 'var(--ob-text-muted)', marginBottom: 8 }}>
                    {blob.mime_type} · {formatSize(blob.total_size)} · {blob.chunk_count} chunks
                  </div>
                  <div style={{ display: 'flex', gap: 6, justifyContent: 'flex-end' }}>
                    <button className="btn btn-icon" title="Preview" onClick={() => setPreviewBlob(blob)}>
                      <Eye size={14} />
                    </button>
                    <button className="btn btn-icon" title={isPinned ? 'Unpin' : 'Pin'} onClick={() => isPinned ? handleUnpin(blob.blob_cid_hex) : handlePin(blob.blob_cid_hex)}>
                      {isPinned ? <PinOff size={14} /> : <Pin size={14} />}
                    </button>
                    <a href={downloadUrl} download={blob.original_name} className="btn btn-icon" title="Download">
                      <Download size={14} />
                    </a>
                    <button className="btn btn-icon" title="Delete" style={{ color: 'var(--ob-error)' }} onClick={() => handleDelete(blob.blob_cid_hex)}>
                      <Trash2 size={14} />
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Preview Modal */}
      {previewBlob && (
        <div style={{
          position: 'fixed', inset: 0, zIndex: 200,
          background: 'rgba(0,0,0,0.7)', backdropFilter: 'blur(8px)',
          display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 40,
        }} onClick={() => setPreviewBlob(null)}>
          <div className="glass-card" style={{ maxWidth: 700, width: '100%', maxHeight: '80vh', overflow: 'auto', padding: 20 }} onClick={e => e.stopPropagation()}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
              <div style={{ fontWeight: 600 }}>{previewBlob.original_name}</div>
              <button className="btn btn-icon" onClick={() => setPreviewBlob(null)}>✕</button>
            </div>
            {previewBlob.mime_type.startsWith('image/') && (
              <img src={`/api/blobs/${previewBlob.blob_cid_hex}/download`} alt={previewBlob.original_name} style={{ width: '100%', borderRadius: 8 }} />
            )}
            {previewBlob.mime_type.startsWith('audio/') && (
              <audio controls src={`/api/blobs/${previewBlob.blob_cid_hex}/download`} style={{ width: '100%' }} />
            )}
            {previewBlob.mime_type.startsWith('video/') && (
              <video controls src={`/api/blobs/${previewBlob.blob_cid_hex}/download`} style={{ width: '100%', maxHeight: 400 }} />
            )}
            {!previewBlob.mime_type.startsWith('image/') && !previewBlob.mime_type.startsWith('audio/') && !previewBlob.mime_type.startsWith('video/') && (
              <div style={{ textAlign: 'center', padding: 40, color: 'var(--ob-text-muted)' }}>
                <Paperclip size={48} style={{ opacity: 0.3, marginBottom: 12 }} />
                <div>Preview not available for this file type.</div>
                <a href={`/api/blobs/${previewBlob.blob_cid_hex}/download`} download={previewBlob.original_name} className="btn btn-primary" style={{ marginTop: 12 }}>
                  <Download size={16} /> Download
                </a>
              </div>
            )}
            <div style={{ marginTop: 12, fontSize: '0.78rem', color: 'var(--ob-text-muted)', display: 'flex', gap: 16 }}>
              <span>CID: {previewBlob.blob_cid_hex.slice(0, 16)}…</span>
              <span>{previewBlob.mime_type}</span>
              <span>{formatSize(previewBlob.total_size)}</span>
              <span>{previewBlob.chunk_count} chunks</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
