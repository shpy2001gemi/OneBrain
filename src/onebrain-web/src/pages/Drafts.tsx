import { useState, useEffect } from 'react';
import { FileEdit, Trash2, Send, Plus, Clock, Edit3 } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { api } from '../api/client';
import type { Draft } from '../api/types';

export function DraftsPage() {
  useTranslation();
  const [drafts, setDrafts] = useState<Draft[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingDraft, setEditingDraft] = useState<Draft | null>(null);
  const [newText, setNewText] = useState('');
  const [newTitle, setNewTitle] = useState('');
  const [showNewForm, setShowNewForm] = useState(false);
  const [publishing, setPublishing] = useState<string | null>(null);
  const [error, setError] = useState('');

  const loadDrafts = () => {
    setError('');
    api.listDrafts()
      .then(d => setDrafts(d.drafts || []))
      .catch((e) => setError(e.message || 'Failed to load drafts'))
      .finally(() => setLoading(false));
  };

  useEffect(() => { loadDrafts(); }, []);

  const handleSave = async () => {
    if (!newText.trim()) return;
    await api.saveDraft(newText.trim(), newTitle.trim() || undefined);
    setNewText('');
    setNewTitle('');
    setShowNewForm(false);
    loadDrafts();
  };

  const handleUpdate = async () => {
    if (!editingDraft || !editingDraft.text.trim()) return;
    await api.updateDraft(editingDraft.id, editingDraft.text, editingDraft.title || undefined);
    setEditingDraft(null);
    loadDrafts();
  };

  const handleDelete = async (draftId: string) => {
    if (!confirm('Delete this draft? This cannot be undone.')) return;
    try {
      await api.deleteDraft(draftId);
      loadDrafts();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Delete failed');
    }
  };

  const handlePublish = async (draftId: string) => {
    setPublishing(draftId);
    try {
      await api.publishDraft(draftId);
      loadDrafts();
    } finally {
      setPublishing(null);
    }
  };

  const formatDate = (ts: number) =>
    ts ? new Date(ts * 1000).toLocaleString() : '—';

  if (loading) {
    return <div className="page" style={{ display: 'flex', justifyContent: 'center', paddingTop: 80 }}><div className="spinner spinner-lg" /></div>;
  }

  return (
    <div className="page">
      {error && (
        <div style={{ padding: '10px 16px', borderRadius: 8, marginBottom: 16, background: 'rgba(239,68,68,0.1)', border: '1px solid rgba(239,68,68,0.3)', color: '#ef4444', fontSize: '0.85rem' }}>
          {error}
        </div>
      )}
      <div className="page-header" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <h1><FileEdit size={28} style={{ display: 'inline', marginRight: 8, verticalAlign: 'middle' }} />Drafts</h1>
          <p style={{ color: 'var(--ob-text-secondary)' }}>
            Save knowledge drafts before encoding to the network
          </p>
        </div>
        <button className="btn btn-primary" onClick={() => setShowNewForm(!showNewForm)}>
          <Plus size={16} /> New Draft
        </button>
      </div>

      {/* New Draft Form */}
      {showNewForm && (
        <div className="glass-card accent-glow animate-in" style={{ marginBottom: 20 }}>
          <input
            className="input"
            placeholder="Title (optional)"
            value={newTitle}
            onChange={e => setNewTitle(e.target.value)}
            style={{ marginBottom: 12 }}
          />
          <textarea
            className="input"
            placeholder="Write your knowledge draft here..."
            value={newText}
            onChange={e => setNewText(e.target.value)}
            rows={6}
            style={{ resize: 'vertical', fontFamily: 'inherit' }}
          />
          <div style={{ display: 'flex', gap: 8, marginTop: 12, justifyContent: 'flex-end' }}>
            <button className="btn" onClick={() => { setShowNewForm(false); setNewText(''); setNewTitle(''); }}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={handleSave} disabled={!newText.trim()}>
              Save Draft
            </button>
          </div>
        </div>
      )}

      {/* Edit Draft Modal */}
      {editingDraft && (
        <div className="glass-card accent-glow animate-in" style={{ marginBottom: 20 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 12 }}>
            <Edit3 size={16} style={{ color: 'var(--ob-accent)' }} />
            <span style={{ fontWeight: 600 }}>Editing Draft</span>
          </div>
          <input
            className="input"
            placeholder="Title"
            value={editingDraft.title}
            onChange={e => setEditingDraft({ ...editingDraft, title: e.target.value })}
            style={{ marginBottom: 12 }}
          />
          <textarea
            className="input"
            value={editingDraft.text}
            onChange={e => setEditingDraft({ ...editingDraft, text: e.target.value })}
            rows={8}
            style={{ resize: 'vertical', fontFamily: 'inherit' }}
          />
          <div style={{ display: 'flex', gap: 8, marginTop: 12, justifyContent: 'flex-end' }}>
            <button className="btn" onClick={() => setEditingDraft(null)}>Cancel</button>
            <button className="btn btn-primary" onClick={handleUpdate}>Save Changes</button>
          </div>
        </div>
      )}

      {/* Draft List */}
      {drafts.length === 0 && !showNewForm ? (
        <div className="glass-card" style={{ textAlign: 'center', padding: '60px 20px', color: 'var(--ob-text-tertiary)' }}>
          <FileEdit size={48} style={{ opacity: 0.3, marginBottom: 12 }} />
          <div>No drafts yet. Click "New Draft" to get started.</div>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {drafts.map(draft => (
            <div key={draft.id} className="glass-card" style={{
              padding: '16px 20px', borderRadius: 12,
              transition: 'all 0.2s',
              borderLeft: '3px solid var(--ob-accent)',
            }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 8 }}>
                <div>
                  <div style={{ fontWeight: 600, fontSize: '1rem', marginBottom: 4 }}>
                    {draft.title || 'Untitled'}
                  </div>
                  <div style={{ display: 'flex', gap: 12, fontSize: '0.75rem', color: 'var(--ob-text-muted)' }}>
                    <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                      <Clock size={10} /> Created: {formatDate(draft.created)}
                    </span>
                    <span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                      <Clock size={10} /> Updated: {formatDate(draft.updated)}
                    </span>
                  </div>
                </div>
                <div style={{ display: 'flex', gap: 6 }}>
                  <button className="btn btn-icon" title="Edit" onClick={() => setEditingDraft({ ...draft })}>
                    <Edit3 size={14} />
                  </button>
                  <button className="btn btn-icon" title="Delete" onClick={() => handleDelete(draft.id)}
                    style={{ color: 'var(--ob-error)' }}>
                    <Trash2 size={14} />
                  </button>
                  <button
                    className="btn btn-primary"
                    onClick={() => handlePublish(draft.id)}
                    disabled={publishing === draft.id}
                    style={{ padding: '6px 12px', fontSize: '0.8rem', display: 'flex', alignItems: 'center', gap: 4 }}
                  >
                    {publishing === draft.id ? (
                      <><div className="spinner" style={{ width: 12, height: 12 }} /> Publishing...</>
                    ) : (
                      <><Send size={12} /> Publish</>
                    )}
                  </button>
                </div>
              </div>
              <div style={{
                padding: '10px 14px', borderRadius: 8, background: 'var(--ob-surface)',
                fontSize: '0.88rem', lineHeight: 1.5, color: 'var(--ob-text-secondary)',
                maxHeight: 120, overflow: 'hidden', whiteSpace: 'pre-wrap',
              }}>
                {draft.text.slice(0, 300)}{draft.text.length > 300 ? '…' : ''}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
