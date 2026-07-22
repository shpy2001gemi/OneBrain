import { Download } from 'lucide-react';
import type { BlobMeta } from '../api/types';
import { formatSize, getMimeIcon } from '../utils/format';

interface Props {
  blob: BlobMeta;
  apiBase?: string;
}

export function FilePreview({ blob, apiBase = '' }: Props) {
  const downloadUrl = `${apiBase}/api/blobs/${blob.blob_cid_hex}/download`;
  const mime = blob.mime_type || '';
  const Icon = getMimeIcon(mime);

  return (
    <div style={{
      borderRadius: 'var(--ob-radius-sm)',
      border: '1px solid var(--ob-glass-border)',
      overflow: 'hidden',
      background: 'var(--ob-surface)',
    }}>
      {/* Inline preview for images */}
      {mime.startsWith('image/') && (
        <div style={{ maxHeight: 200, overflow: 'hidden', display: 'flex', justifyContent: 'center', background: 'rgba(0,0,0,0.3)' }}>
          <img
            src={downloadUrl}
            alt={blob.original_name}
            style={{ maxWidth: '100%', maxHeight: 200, objectFit: 'contain' }}
            loading="lazy"
          />
        </div>
      )}

      {/* Inline preview for audio */}
      {mime.startsWith('audio/') && (
        <div style={{ padding: '8px 12px' }}>
          <audio controls src={downloadUrl} style={{ width: '100%', height: 32 }} preload="none" />
        </div>
      )}

      {/* Inline preview for video */}
      {mime.startsWith('video/') && (
        <div style={{ maxHeight: 200, overflow: 'hidden' }}>
          <video controls src={downloadUrl} style={{ width: '100%', maxHeight: 200 }} preload="none" />
        </div>
      )}

      {/* Info bar */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 8, padding: '8px 12px',
        borderTop: (mime.startsWith('image/') || mime.startsWith('audio/') || mime.startsWith('video/'))
          ? '1px solid var(--ob-glass-border)' : 'none',
      }}>
        <Icon size={16} style={{ color: 'var(--ob-accent)', flexShrink: 0 }} />
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{
            fontSize: '0.82rem', fontWeight: 500,
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
          }}>
            {blob.original_name}
          </div>
          <div style={{ fontSize: '0.72rem', color: 'var(--ob-text-muted)' }}>
            {mime} · {formatSize(blob.total_size)}
          </div>
        </div>
        <a
          href={downloadUrl}
          download={blob.original_name}
          style={{
            display: 'flex', alignItems: 'center', padding: '4px 8px',
            borderRadius: 'var(--ob-radius-sm)',
            background: 'rgba(6, 182, 212, 0.1)', color: 'var(--ob-accent)',
            border: '1px solid rgba(6, 182, 212, 0.2)',
            textDecoration: 'none', fontSize: '0.75rem', gap: 4,
            transition: 'all 0.2s',
          }}
          aria-label={`Download ${blob.original_name}`}
        >
          <Download size={12} /> Download
        </a>
      </div>
    </div>
  );
}
