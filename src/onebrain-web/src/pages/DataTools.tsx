import { useState, useRef } from 'react';
import { api } from '../api/client';
import type { ImportResult, BlobMeta, BulkDeleteResult } from '../api/types';
import { ALL_GENE_TYPES } from '../api/types';

// ─── Shared inline styles ────────────────────────────────
const styles = {
  grid: {
    display: 'grid',
    gridTemplateColumns: '1fr 1fr',
    gap: 'var(--ob-gap-lg)',
  } as React.CSSProperties,

  cardTitle: {
    fontSize: '1rem',
    fontWeight: 600,
    marginBottom: 16,
    display: 'flex',
    alignItems: 'center',
    gap: 10,
  } as React.CSSProperties,

  cardIcon: {
    fontSize: '1.25rem',
    width: 36,
    height: 36,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 'var(--ob-radius-md)',
    background: 'rgba(79, 195, 247, 0.08)',
    flexShrink: 0,
  } as React.CSSProperties,

  cardBody: {
    display: 'flex',
    flexDirection: 'column' as const,
    gap: 12,
  } as React.CSSProperties,

  desc: {
    fontSize: '0.8rem',
    color: 'var(--ob-text-secondary)',
    lineHeight: 1.5,
    margin: 0,
  } as React.CSSProperties,

  fileInput: {
    fontSize: '0.82rem',
    color: 'var(--ob-text-secondary)',
    padding: '8px 0',
  } as React.CSSProperties,

  selectWrap: {
    display: 'flex',
    alignItems: 'center',
    gap: 10,
  } as React.CSSProperties,

  label: {
    fontSize: '0.82rem',
    color: 'var(--ob-text-secondary)',
    fontWeight: 500,
    minWidth: 54,
  } as React.CSSProperties,

  resultBox: {
    fontSize: '0.8rem',
    padding: '10px 14px',
    borderRadius: 'var(--ob-radius-md)',
    background: 'rgba(255,255,255,0.03)',
    border: '1px solid var(--ob-glass-border)',
    color: 'var(--ob-text-primary)',
    display: 'flex',
    flexDirection: 'column' as const,
    gap: 4,
  } as React.CSSProperties,

  resultRow: {
    display: 'flex',
    justifyContent: 'space-between',
  } as React.CSSProperties,

  resultLabel: {
    color: 'var(--ob-text-secondary)',
  } as React.CSSProperties,

  dangerBtn: {
    background: 'rgba(239, 68, 68, 0.12)',
    color: 'var(--ob-error)',
    border: '1px solid rgba(239, 68, 68, 0.25)',
  } as React.CSSProperties,

  message: (type: 'success' | 'error'): React.CSSProperties => ({
    padding: '10px 16px',
    borderRadius: 'var(--ob-radius-md)',
    marginBottom: 16,
    background: type === 'success' ? 'rgba(76, 175, 80, 0.12)' : 'rgba(239, 68, 68, 0.12)',
    color: type === 'success' ? 'var(--ob-success)' : 'var(--ob-error)',
    border: `1px solid ${type === 'success' ? 'rgba(76, 175, 80, 0.25)' : 'rgba(239, 68, 68, 0.25)'}`,
    display: 'flex',
    alignItems: 'center',
    gap: 8,
    fontSize: '0.85rem',
    fontWeight: 500,
    transition: 'var(--ob-transition)',
  }),
};

export function DataToolsPage() {
  // ─── State ─────────────────────────────────────────────
  const [exportFormat, setExportFormat] = useState<'json' | 'csv'>('json');
  const [exporting, setExporting] = useState(false);

  const [importing, setImporting] = useState(false);
  const [importResult, setImportResult] = useState<ImportResult | null>(null);
  const importFileRef = useRef<HTMLInputElement>(null);

  const [backupPassword, setBackupPassword] = useState('');
  const [creatingBackup, setCreatingBackup] = useState(false);

  const [restorePassword, setRestorePassword] = useState('');
  const [restoring, setRestoring] = useState(false);
  const restoreFileRef = useRef<HTMLInputElement>(null);

  const [uploading, setUploading] = useState(false);
  const [uploadResult, setUploadResult] = useState<BlobMeta | null>(null);
  const blobFileRef = useRef<HTMLInputElement>(null);

  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkDeleteResult, setBulkDeleteResult] = useState<BulkDeleteResult | null>(null);
  const [bulkGeneType, setBulkGeneType] = useState('');
  const [bulkBefore, setBulkBefore] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);

  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  // ─── Helpers ───────────────────────────────────────────
  const showMessage = (type: 'success' | 'error', text: string) => {
    setMessage({ type, text });
    setTimeout(() => setMessage(null), 4000);
  };

  // ─── Handlers ──────────────────────────────────────────
  const handleExport = async () => {
    setExporting(true);
    try {
      const blob = await api.exportKus(exportFormat);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `onebrain-export-${Date.now()}.${exportFormat}`;
      a.click();
      URL.revokeObjectURL(url);
      showMessage('success', `Exported KUs as ${exportFormat.toUpperCase()} successfully`);
    } catch (err: any) {
      showMessage('error', err.message || 'Export failed');
    } finally {
      setExporting(false);
    }
  };

  const handleImport = async () => {
    const file = importFileRef.current?.files?.[0];
    if (!file) { showMessage('error', 'Please select a file first'); return; }
    setImporting(true);
    setImportResult(null);
    try {
      const result = await api.importKus(file);
      setImportResult(result);
      showMessage('success', `Imported ${result.imported} KUs`);
    } catch (err: any) {
      showMessage('error', err.message || 'Import failed');
    } finally {
      setImporting(false);
    }
  };

  const handleCreateBackup = async () => {
    setCreatingBackup(true);
    try {
      const blob = await api.createBackup(backupPassword);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `onebrain-backup-${Date.now()}.obk`;
      a.click();
      URL.revokeObjectURL(url);
      setBackupPassword('');
      showMessage('success', 'Backup created and downloaded');
    } catch (err: any) {
      showMessage('error', err.message || 'Backup failed');
    } finally {
      setCreatingBackup(false);
    }
  };

  const handleRestore = async () => {
    const file = restoreFileRef.current?.files?.[0];
    if (!file) { showMessage('error', 'Please select a backup file'); return; }
    setRestoring(true);
    try {
      await api.restoreBackup(file, restorePassword);
      setRestorePassword('');
      showMessage('success', 'Backup restored successfully');
    } catch (err: any) {
      showMessage('error', err.message || 'Restore failed');
    } finally {
      setRestoring(false);
    }
  };

  const handleUploadBlob = async () => {
    const file = blobFileRef.current?.files?.[0];
    if (!file) { showMessage('error', 'Please select a file to upload'); return; }
    setUploading(true);
    setUploadResult(null);
    try {
      const meta = await api.uploadBlob(file);
      setUploadResult(meta);
      showMessage('success', `Blob uploaded: ${meta.original_name}`);
    } catch (err: any) {
      showMessage('error', err.message || 'Upload failed');
    } finally {
      setUploading(false);
    }
  };

  const handleBulkDelete = async () => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      return;
    }
    setBulkDeleting(true);
    setBulkDeleteResult(null);
    setConfirmDelete(false);
    try {
      const beforeTs = bulkBefore ? Math.floor(new Date(bulkBefore).getTime() / 1000) : undefined;
      const result = await api.bulkDeleteKus(bulkGeneType || undefined, beforeTs);
      setBulkDeleteResult(result);
      showMessage('success', `Deleted ${result.deleted} KUs`);
    } catch (err: any) {
      showMessage('error', err.message || 'Bulk delete failed');
    } finally {
      setBulkDeleting(false);
    }
  };

  const formatBytes = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  };

  // ─── Render ────────────────────────────────────────────
  return (
    <div className="page">
      <div className="page-header">
        <h1>Data Tools</h1>
        <p>Export, import, backup, restore, and manage your knowledge data</p>
      </div>

      {/* Global message banner */}
      {message && (
        <div style={styles.message(message.type)}>
          <span>{message.type === 'success' ? '✅' : '❌'}</span>
          {message.text}
        </div>
      )}

      <div style={styles.grid}>
        {/* ── Card 1: Export KUs ───────────────────────────── */}
        <div className="glass-card animate-in">
          <h3 style={styles.cardTitle}>
            <span style={styles.cardIcon}>📤</span>
            Export KUs
          </h3>
          <div style={styles.cardBody}>
            <p style={styles.desc}>
              Download all your Knowledge Units as a portable file.
            </p>
            <div style={styles.selectWrap}>
              <span style={styles.label}>Format</span>
              <select
                className="input"
                value={exportFormat}
                onChange={e => setExportFormat(e.target.value as 'json' | 'csv')}
                style={{ flex: 1 }}
              >
                <option value="json">JSON</option>
                <option value="csv">CSV</option>
              </select>
            </div>
            <button
              className="btn btn-primary"
              onClick={handleExport}
              disabled={exporting}
              style={{ marginTop: 4 }}
            >
              {exporting ? 'Exporting…' : `Download ${exportFormat.toUpperCase()}`}
            </button>
          </div>
        </div>

        {/* ── Card 2: Import KUs ──────────────────────────── */}
        <div className="glass-card animate-in">
          <h3 style={styles.cardTitle}>
            <span style={styles.cardIcon}>📥</span>
            Import KUs
          </h3>
          <div style={styles.cardBody}>
            <p style={styles.desc}>
              Import Knowledge Units from a JSON or CSV export file.
            </p>
            <input
              ref={importFileRef}
              type="file"
              accept=".json,.csv"
              style={styles.fileInput}
            />
            <button
              className="btn btn-primary"
              onClick={handleImport}
              disabled={importing}
            >
              {importing ? 'Importing…' : 'Import File'}
            </button>
            {importResult && (
              <div style={styles.resultBox}>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>Imported</span>
                  <span style={{ color: 'var(--ob-success)' }}>{importResult.imported}</span>
                </div>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>Skipped</span>
                  <span>{importResult.skipped}</span>
                </div>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>Errors</span>
                  <span style={{ color: importResult.errors > 0 ? 'var(--ob-error)' : 'inherit' }}>
                    {importResult.errors}
                  </span>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* ── Card 3: Backup ──────────────────────────────── */}
        <div className="glass-card animate-in">
          <h3 style={styles.cardTitle}>
            <span style={styles.cardIcon}>💾</span>
            Backup
          </h3>
          <div style={styles.cardBody}>
            <p style={styles.desc}>
              Create an encrypted backup of your entire node data. Password is optional but recommended.
            </p>
            <input
              className="input"
              type="password"
              placeholder="Encryption password (optional)"
              value={backupPassword}
              onChange={e => setBackupPassword(e.target.value)}
            />
            <button
              className="btn btn-primary"
              onClick={handleCreateBackup}
              disabled={creatingBackup}
            >
              {creatingBackup ? 'Creating Backup…' : 'Create & Download Backup'}
            </button>
          </div>
        </div>

        {/* ── Card 4: Restore ─────────────────────────────── */}
        <div className="glass-card animate-in">
          <h3 style={styles.cardTitle}>
            <span style={styles.cardIcon}>🔄</span>
            Restore
          </h3>
          <div style={styles.cardBody}>
            <p style={styles.desc}>
              Restore your node from a previously created backup file.
            </p>
            <input
              ref={restoreFileRef}
              type="file"
              accept=".obk,.bak"
              style={styles.fileInput}
            />
            <input
              className="input"
              type="password"
              placeholder="Backup password"
              value={restorePassword}
              onChange={e => setRestorePassword(e.target.value)}
            />
            <button
              className="btn btn-primary"
              onClick={handleRestore}
              disabled={restoring}
            >
              {restoring ? 'Restoring…' : 'Restore Backup'}
            </button>
          </div>
        </div>

        {/* ── Card 5: Blob Upload ─────────────────────────── */}
        <div className="glass-card animate-in">
          <h3 style={styles.cardTitle}>
            <span style={styles.cardIcon}>☁️</span>
            Blob Upload
          </h3>
          <div style={styles.cardBody}>
            <p style={styles.desc}>
              Upload binary files (images, PDFs, etc.) to the OneBrain blob store.
            </p>
            <input
              ref={blobFileRef}
              type="file"
              style={styles.fileInput}
            />
            <button
              className="btn btn-primary"
              onClick={handleUploadBlob}
              disabled={uploading}
            >
              {uploading ? 'Uploading…' : 'Upload Blob'}
            </button>
            {uploadResult && (
              <div style={styles.resultBox}>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>Name</span>
                  <span style={{ maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {uploadResult.original_name}
                  </span>
                </div>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>MIME</span>
                  <span>{uploadResult.mime_type}</span>
                </div>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>Size</span>
                  <span>{formatBytes(uploadResult.total_size)}</span>
                </div>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>CID</span>
                  <span style={{
                    fontFamily: 'monospace',
                    fontSize: '0.72rem',
                    color: 'var(--ob-accent-light)',
                    maxWidth: 140,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}>
                    {uploadResult.blob_cid_hex}
                  </span>
                </div>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>Chunks</span>
                  <span>{uploadResult.chunk_count}</span>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* ── Card 6: Bulk Delete ─────────────────────────── */}
        <div className="glass-card animate-in">
          <h3 style={styles.cardTitle}>
            <span style={{ ...styles.cardIcon, background: 'rgba(239, 68, 68, 0.08)' }}>🗑️</span>
            Bulk Delete
          </h3>
          <div style={styles.cardBody}>
            <p style={styles.desc}>
              Delete multiple KUs at once. Filter by gene type and/or creation date.
              <span style={{ color: 'var(--ob-error)', fontWeight: 600 }}> This action is irreversible.</span>
            </p>
            <div style={styles.selectWrap}>
              <span style={styles.label}>Type</span>
              <select
                className="input"
                value={bulkGeneType}
                onChange={e => { setBulkGeneType(e.target.value); setConfirmDelete(false); }}
                style={{ flex: 1 }}
              >
                <option value="">All Types</option>
                {ALL_GENE_TYPES.map(gt => (
                  <option key={gt} value={gt}>{gt}</option>
                ))}
              </select>
            </div>
            <div style={styles.selectWrap}>
              <span style={styles.label}>Before</span>
              <input
                className="input"
                type="date"
                value={bulkBefore}
                onChange={e => { setBulkBefore(e.target.value); setConfirmDelete(false); }}
                style={{ flex: 1 }}
              />
            </div>
            <button
              className="btn"
              style={{
                ...styles.dangerBtn,
                opacity: bulkDeleting ? 0.6 : 1,
                cursor: bulkDeleting ? 'not-allowed' : 'pointer',
              }}
              onClick={handleBulkDelete}
              disabled={bulkDeleting}
            >
              {bulkDeleting
                ? 'Deleting…'
                : confirmDelete
                  ? '⚠️ Click again to confirm deletion'
                  : 'Delete Matching KUs'}
            </button>
            {bulkDeleteResult && (
              <div style={styles.resultBox}>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>Deleted</span>
                  <span style={{ color: 'var(--ob-error)' }}>{bulkDeleteResult.deleted}</span>
                </div>
                <div style={styles.resultRow}>
                  <span style={styles.resultLabel}>Skipped</span>
                  <span>{bulkDeleteResult.skipped}</span>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
