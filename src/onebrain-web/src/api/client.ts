import type {
  ApiResponse, StatusResponse, KuListResponse,
  KuDetail, KuListItem, EncodeResult, ChatResponse, WalletInfo,
  WalletTransaction, UserProfile, ConfigView,
  AiHealthInfo, PeerView, BlobMeta, FollowedNode, PeerProfile,
  DeviceInfo, SyncStatus, WatchInfo, ImportResult, BulkDeleteResult,
  Draft, NeighborInfo, VNextMeta, VNextResult, NeedPrepareRequest,
  PreparedNeed, NeedView, NeedPage, VNextBudget, MatchPage,
  PublicUsePrepareRequest, PreparedPublicUse, PublicationView,
  MetabolicEvidenceView, VNextRuntimeStatus,
} from './types';
import { logDebug } from '../components/DebugConsole';
import { isTauri, getApiConfig } from './tauri';

let API_BASE = import.meta.env.VITE_API_BASE || 'http://127.0.0.1:4280';
let TOKEN = localStorage.getItem('ob_api_token') || '';
let configReady: Promise<void> | null = null;

/** Get the current API base URL (e.g. http://127.0.0.1:4280). */
export function getApiBase() { return API_BASE; }

/** Get the WebSocket base URL (e.g. ws://127.0.0.1:4280). */
export function getWsBase() { return API_BASE.replace(/^http/, 'ws'); }

async function initConfig() {
  if (isTauri()) {
    const cfg = await getApiConfig();
    API_BASE = cfg.baseUrl;
    TOKEN = cfg.token;
  }
}

function ensureConfig(): Promise<void> {
  if (!configReady) {
    configReady = initConfig();
  }
  return configReady;
}

function getToken(): string {
  return TOKEN || localStorage.getItem('ob_api_token') || '';
}

/** Shared local authentication configuration; KU payloads bypass debug logging. */
export async function getPrivateApiConnection() {
  await ensureConfig();
  return { baseUrl: API_BASE, token: getToken() };
}

export function setToken(token: string) {
  TOKEN = token;
  localStorage.setItem('ob_api_token', token);
}

// Used by future logout flow
export function clearToken() {
  TOKEN = '';
  localStorage.removeItem('ob_api_token');
}

async function request<T>(path: string, opts: RequestInit = {}): Promise<T> {
  await ensureConfig();
  const method = (opts.method || 'GET').toUpperCase();
  const start = performance.now();

  logDebug({ type: 'request', method, path, body: opts.body ? String(opts.body) : undefined });

  try {
    const res = await fetch(`${API_BASE}${path}`, {
      ...opts,
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${getToken()}`,
        ...opts.headers,
      },
    });
    const duration = Math.round(performance.now() - start);
    const json = await res.json() as ApiResponse<T>;

    if (!json.ok) {
      logDebug({
        type: 'error', method, path, status: res.status, duration,
        message: json.error.message || json.error.code,
        body: JSON.stringify(json, null, 2),
      });
      throw new Error(json.error.message || json.error.code);
    }

    logDebug({
      type: 'response', method, path, status: res.status, duration,
      body: JSON.stringify(json.data, null, 2).slice(0, 2000),
    });
    return json.data;
  } catch (err: unknown) {
    const duration = Math.round(performance.now() - start);
    const errMsg = err instanceof Error ? err.message : String(err);
    if (errMsg && !errMsg.includes('AUTH_')) {
      logDebug({
        type: 'error', method, path, duration,
        message: errMsg,
      });
    }
    throw err;
  }
}

type VNextEnvelope<T> =
  | { ok: true; profile: string; data: T; meta: VNextMeta }
  | {
      ok: false;
      profile: string;
      error: {
        code: string;
        message: string;
        retryable: boolean;
        limitations: string[];
      };
      meta: VNextMeta;
    };

export class VNextApiError extends Error {
  readonly code: string;
  readonly meta: VNextMeta;
  readonly retryable: boolean;

  constructor(
    message: string,
    code: string,
    meta: VNextMeta,
    retryable: boolean,
  ) {
    super(message);
    this.name = 'VNextApiError';
    this.code = code;
    this.meta = meta;
    this.retryable = retryable;
  }
}

function vnextDebugBody(path: string, body: BodyInit | null | undefined) {
  if (!body) return undefined;
  if (path === '/api/vnext/pomv/public-use/confirm') {
    return '{"intent_cid":"[BOUND]","single_use_receipt":"[REDACTED]"}';
  }
  if (path === '/api/vnext/kql/needs/prepare') {
    try {
      const parsed = JSON.parse(String(body)) as Record<string, unknown>;
      parsed.local_query = '[PRIVATE_REDACTED]';
      return JSON.stringify(parsed);
    } catch {
      return '[PRIVATE_REDACTED]';
    }
  }
  return String(body);
}

async function vnextRequest<T>(
  path: string,
  opts: RequestInit = {},
): Promise<VNextResult<T>> {
  await ensureConfig();
  const method = (opts.method || 'GET').toUpperCase();
  const start = performance.now();
  logDebug({ type: 'request', method, path, body: vnextDebugBody(path, opts.body) });

  const res = await fetch(`${API_BASE}${path}`, {
    ...opts,
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${getToken()}`,
      ...opts.headers,
    },
  });
  const duration = Math.round(performance.now() - start);
  const json = await res.json() as VNextEnvelope<T>;
  if (!json.ok) {
    logDebug({
      type: 'error',
      method,
      path,
      status: res.status,
      duration,
      message: json.error.message,
      body: JSON.stringify(json, null, 2),
    });
    throw new VNextApiError(
      json.error.message,
      json.error.code,
      json.meta,
      json.error.retryable,
    );
  }
  logDebug({
    type: 'response',
    method,
    path,
    status: res.status,
    duration,
    body: JSON.stringify(json, null, 2).slice(0, 2000),
  });
  return { data: json.data, meta: json.meta };
}

// ─── Identity ────────────────────
export const api = {

  // ─── Knowledge ──────────────────
  encode: (text: string) =>
    request<EncodeResult>('/api/encode', {
      method: 'POST', body: JSON.stringify({ text }),
    }),
  listKus: (page = 1, limit = 20, geneType?: string, sort = 'created') => {
    const params = new URLSearchParams({ page: String(page), limit: String(limit), sort });
    if (geneType) params.set('gene_type', geneType);
    return request<KuListResponse>(`/api/kus?${params}`);
  },
  getKu: (cid: string) => request<KuDetail>(`/api/kus/${cid}`),
  deleteKu: (cid: string) => request<{ deleted: boolean }>(`/api/kus/${cid}`, { method: 'DELETE' }),
  search: (query: string, limit = 10) =>
    request<{ results: KuListItem[]; total: number }>('/api/search', {
      method: 'POST', body: JSON.stringify({ query, limit }),
    }),
  kql: (query: string) =>
    request<{ results: unknown[] }>('/api/kql', {
      method: 'POST', body: JSON.stringify({ query }),
    }),
  searchSuggest: (q: string, limit = 5) =>
    request<{ tags: string[]; gene_types: string[]; kus: KuListItem[] }>(`/api/search/suggest?q=${encodeURIComponent(q)}&limit=${limit}`),

  // ─── Chat ───────────────────────
  chat: (message: string) =>
    request<ChatResponse>('/api/chat', {
      method: 'POST', body: JSON.stringify({ message }),
    }),

  // ─── Network ────────────────────
  getStatus: () => request<StatusResponse>('/api/status'),
  getPeers: () => request<{ peers: PeerView[]; count: number }>('/api/peers'),
  connectPeer: (address: string) =>
    request<{ connected: boolean }>('/api/peers/connect', {
      method: 'POST', body: JSON.stringify({ address }),
    }),

  // ─── Graph ──────────────────────
  getGraph: (cid: string, depth = 2) =>
    request<{ cid_hex: string; neighbors: NeighborInfo[] }>(`/api/graph/${cid}?depth=${depth}`),


  // ─── Wallet ─────────────────────
  getWallet: () => request<WalletInfo>('/api/wallet'),
  getWalletHistory: (limit = 50) =>
    request<WalletTransaction[]>(`/api/wallet/history?limit=${limit}`),
  stake: (amount: number) =>
    request<WalletInfo>('/api/wallet/stake', { method: 'POST', body: JSON.stringify({ amount }) }),
  unstake: (amount: number) =>
    request<WalletInfo>('/api/wallet/unstake', { method: 'POST', body: JSON.stringify({ amount }) }),

  // ─── Profile & Settings ─────────
  getProfile: () => request<UserProfile>('/api/profile'),
  updateProfile: (updates: Partial<{ display_name: string; language: string; response_style: string }>) =>
    request<UserProfile>('/api/profile', {
      method: 'PATCH', body: JSON.stringify(updates),
    }),
  getSettings: () => request<ConfigView>('/api/settings'),
  updateSettings: (updates: Partial<{ name: string; ollama_url: string; model: string }>) =>
    request<ConfigView>('/api/settings', {
      method: 'PATCH', body: JSON.stringify(updates),
    }),

  // ─── AI ─────────────────────────
  aiStatus: () => request<AiHealthInfo>('/api/ai/status'),
  switchModel: (modelName: string) =>
    request<{ model: string; switched: boolean }>('/api/ai/model', {
      method: 'POST', body: JSON.stringify({ model_name: modelName }),
    }),


  // ─── Phase 1: Knowledge Management ─────
  deprecateKu: (cid: string) =>
    request<{ deprecated: boolean; cid_hex: string }>(`/api/kus/${cid}/deprecate`, { method: 'POST' }),
  // ─── Drafts ─────────────────────────────
  saveDraft: (text: string, title?: string) =>
    request<Draft>('/api/drafts', {
      method: 'POST', body: JSON.stringify({ text, title }),
    }),
  listDrafts: () =>
    request<{ drafts: Draft[]; total: number }>('/api/drafts'),

  updateDraft: (draftId: string, text: string, title?: string) =>
    request<Draft>(`/api/drafts/${draftId}`, {
      method: 'PUT', body: JSON.stringify({ text, title }),
    }),
  deleteDraft: (draftId: string) =>
    request<{ deleted: boolean }>(`/api/drafts/${draftId}`, { method: 'DELETE' }),
  publishDraft: (draftId: string) =>
    request<EncodeResult>(`/api/drafts/${draftId}/publish`, { method: 'POST' }),

  // ─── Tags ──────────────────────────────
  addTag: (cid: string, tag: string) =>
    request<{ added: boolean }>(`/api/kus/${cid}/tags`, {
      method: 'POST', body: JSON.stringify({ tag }),
    }),
  removeTag: (cid: string, tag: string) =>
    request<{ removed: boolean }>(`/api/kus/${cid}/tags/${encodeURIComponent(tag)}`, { method: 'DELETE' }),
  listTags: () =>
    request<{ tags: string[]; count: number }>('/api/tags'),
  getKuTags: (cid: string) =>
    request<{ tags: string[]; cid_hex: string }>(`/api/kus/${cid}/tags`),

  // ─── Pin/Favorite KUs ─────────────────
  pinKu: (cid: string) =>
    request<{ pinned: boolean }>(`/api/kus/${cid}/pin`, { method: 'POST' }),
  unpinKu: (cid: string) =>
    request<{ unpinned: boolean }>(`/api/kus/${cid}/pin`, { method: 'DELETE' }),
  listPinnedKus: () =>
    request<KuListItem[]>('/api/kus/pinned'),

  // ─── Social & Discovery ───────────────
  followNode: (nodeId: string) =>
    request<{ followed: boolean }>(`/api/follow/${nodeId}`, { method: 'POST' }),
  unfollowNode: (nodeId: string) =>
    request<{ unfollowed: boolean }>(`/api/follow/${nodeId}`, { method: 'DELETE' }),
  listFollowing: () =>
    request<FollowedNode[]>('/api/following'),
  getPeerProfile: (nodeId: string) =>
    request<PeerProfile>(`/api/nodes/${nodeId}/profile`),

  // ─── Multi-Device ─────────────────────
  listDevices: () =>
    request<DeviceInfo[]>('/api/devices'),
  syncStatus: () =>
    request<SyncStatus>('/api/sync/status'),

  // ─── Bulk Operations ──────────────────
  bulkDeleteKus: (geneType?: string, beforeTimestamp?: number) =>
    request<BulkDeleteResult>('/api/kus/bulk-delete', {
      method: 'POST',
      body: JSON.stringify({ gene_type: geneType, before_timestamp: beforeTimestamp }),
    }),

  // ─── Watch (Standing Queries) ─────────
  createWatch: (query: string) =>
    request<{ watch_id: string; query: string }>('/api/watch', {
      method: 'POST', body: JSON.stringify({ query }),
    }),
  listWatches: () =>
    request<WatchInfo[]>('/api/watch'),
  deleteWatch: (watchId: string) =>
    request<{ deleted: boolean }>(`/api/watch/${watchId}`, { method: 'DELETE' }),

  // ─── Blob Operations ───────────────────
  listBlobs: () =>
    request<{ blobs: BlobMeta[] }>('/api/blobs'),


  // ─── Blob Extensions ──────────────────
  addBlobKuRef: (blobCid: string, kuCid: string) =>
    request<{ linked: boolean }>(`/api/blobs/${blobCid}/refs`, {
      method: 'POST', body: JSON.stringify({ ku_cid: kuCid }),
    }),
  pinBlob: (cid: string) =>
    request<{ pinned: boolean }>(`/api/blobs/${cid}/pin`, { method: 'POST' }),
  unpinBlob: (cid: string) =>
    request<{ unpinned: boolean }>(`/api/blobs/${cid}/unpin`, { method: 'POST' }),

  // ─── Data Portability ─────────────────
  exportKus: async (format: 'json' | 'csv' = 'json') => {
    await ensureConfig();
    return fetch(`${API_BASE}/api/export?format=${format}`, {
      headers: { 'Authorization': `Bearer ${getToken()}` },
    }).then(r => r.blob());
  },
  importKus: async (file: File) => {
    await ensureConfig();
    const fd = new FormData();
    fd.append('file', file);
    return fetch(`${API_BASE}/api/import`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${getToken()}` },
      body: fd,
    }).then(r => r.json()).then(j => j.data as ImportResult);
  },
  createBackup: async (password = '') => {
    await ensureConfig();
    return fetch(`${API_BASE}/api/backup`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${getToken()}`,
      },
      body: JSON.stringify({ password }),
    }).then(r => r.blob());
  },
  restoreBackup: async (file: File, password = '') => {
    await ensureConfig();
    const fd = new FormData();
    fd.append('file', file);
    fd.append('password', password);
    return fetch(`${API_BASE}/api/restore`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${getToken()}` },
      body: fd,
    }).then(r => r.json()).then(j => j.data as { restored: boolean });
  },

  // ─── Blob Upload & Download ───────────
  uploadBlob: async (file: File) => {
    await ensureConfig();
    const fd = new FormData();
    fd.append('file', file);
    return fetch(`${API_BASE}/api/blobs/upload`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${getToken()}` },
      body: fd,
    }).then(r => r.json()).then(j => j.data as BlobMeta);
  },

  deleteBlob: (cid: string) =>
    request<{ deleted: boolean }>(`/api/blobs/${cid}`, { method: 'DELETE' }),

  // ─── Tier C: Search History ────────────
  recordSearch: (query: string, resultCount: number) =>
    request<{ recorded: boolean }>('/api/search-history', {
      method: 'POST', body: JSON.stringify({ query, result_count: resultCount }),
    }),
  listSearchHistory: (limit = 50) =>
    request<{ history: Array<{ id: string; query: string; result_count: number; timestamp: number }> }>(`/api/search-history?limit=${limit}`),


  // ─── Tier C: Notification Preferences ──
  getNotificationPrefs: () =>
    request<{ encode_complete: boolean; peer_connected: boolean; sync_complete: boolean; watch_match: boolean; errors: boolean }>('/api/notification-prefs'),
  setNotificationPrefs: (prefs: { encode_complete: boolean; peer_connected: boolean; sync_complete: boolean; watch_match: boolean; errors: boolean }) =>
    request<typeof prefs>('/api/notification-prefs', { method: 'PUT', body: JSON.stringify(prefs) }),

  // ─── Tier C: Saved Searches ────────────
  saveSearch: (name: string, query: string, isKql = false) =>
    request<{ id: string; name: string; query: string; is_kql: boolean; created_at: number }>('/api/saved-searches', {
      method: 'POST', body: JSON.stringify({ name, query, is_kql: isKql }),
    }),
  listSavedSearches: () =>
    request<{ saved_searches: Array<{ id: string; name: string; query: string; is_kql: boolean; created_at: number }> }>('/api/saved-searches'),


  // ─── Tier C: Collections ───────────────
  createCollection: (name: string, description = '') =>
    request<{ id: string; name: string; description: string; ku_cids: string[]; created_at: number; updated_at: number }>('/api/collections', {
      method: 'POST', body: JSON.stringify({ name, description }),
    }),
  listCollections: () =>
    request<{ collections: Array<{ id: string; name: string; description: string; ku_cids: string[]; created_at: number; updated_at: number }> }>('/api/collections'),

  deleteCollection: (id: string) =>
    request<{ deleted: boolean }>(`/api/collections/${id}`, { method: 'DELETE' }),

  removeFromCollection: (collectionId: string, cidHex: string) =>
    request<{ removed: boolean }>(`/api/collections/${collectionId}/kus/${cidHex}`, { method: 'DELETE' }),

  // ─── Tier C: KU Version Chain ──────────
  getVersionChain: (cid: string) =>
    request<{ versions: Array<{ cid_hex: string; gene_type: string; preview: string; version: number; created: number }> }>(`/api/kus/${cid}/versions`),

  // ─── Tier C: Trending & Recommendations ─
  getTrending: (limit = 10) =>
    request<{ trending: Array<{ ku: KuListItem; trend_score: number; reason: string }> }>(`/api/trending?limit=${limit}`),
  getRecommendations: (limit = 10) =>
    request<{ recommendations: Array<{ ku: KuListItem; relevance: number; reason: string }> }>(`/api/recommendations?limit=${limit}`),

  // ─── Tier C: Analytics ─────────────────
  getAnalytics: () =>
    request<{ total_kus: number; kus_by_type: [string, number][]; avg_pomv: number; pomv_profile: 'legacy_local_pomv_scalar_v1'; pomv_is_economic: false; avg_trust: number; total_wire_size: number; total_bonds: number; kus_last_24h: number; kus_last_7d: number; top_gene_type: string; verified_self: number; verified_partial: number; verified_full: number; verification_rate: number }>('/api/analytics'),

  // ─── Tier C: Domain Taxonomy ───────────
  listDomains: () =>
    request<{ domains: Array<{ name: string; ku_count: number; avg_pomv: number; pomv_profile: 'legacy_local_pomv_scalar_v1'; pomv_is_economic: false; example_cids: string[] }> }>('/api/domains'),
  kusByDomain: (domain: string, page = 1, limit = 20) =>
    request<{ kus: KuListItem[]; total: number; page: number }>(`/api/domains/${encodeURIComponent(domain)}/kus?page=${page}&limit=${limit}`),

  // ─── vNext: private one-hop discovery ──────────────────────────────────
  prepareNeed: (input: NeedPrepareRequest) =>
    vnextRequest<PreparedNeed>('/api/vnext/kql/needs/prepare', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  activateNeed: (intentCid: string, idempotencyKey: string) =>
    vnextRequest<NeedView>('/api/vnext/kql/needs', {
      method: 'POST',
      body: JSON.stringify({
        intent_cid: intentCid,
        idempotency_key: idempotencyKey,
      }),
    }),
  listNeeds: (continuation?: string, limit = 100) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (continuation) params.set('continuation', continuation);
    return vnextRequest<NeedPage>(`/api/vnext/kql/needs?${params}`);
  },
  getNeed: (id: string) =>
    vnextRequest<NeedView>(`/api/vnext/kql/needs/${encodeURIComponent(id)}`),
  scanNeed: (
    id: string,
    budget: VNextBudget,
    idempotencyKey: string,
    continuation?: string,
  ) =>
    vnextRequest<NeedView>(`/api/vnext/kql/needs/${encodeURIComponent(id)}/scan`, {
      method: 'POST',
      body: JSON.stringify({
        budget,
        continuation: continuation || null,
        idempotency_key: idempotencyKey,
      }),
    }),
  listNeedMatches: (id: string, continuation?: string, limit = 100) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (continuation) params.set('continuation', continuation);
    return vnextRequest<MatchPage>(
      `/api/vnext/kql/needs/${encodeURIComponent(id)}/matches?${params}`,
    );
  },
  retireNeed: (id: string) =>
    vnextRequest<NeedView>(`/api/vnext/kql/needs/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    }),

  // ─── vNext: Public Use and Metabolic Evidence View ─────────────────────
  preparePublicUse: (input: PublicUsePrepareRequest) =>
    vnextRequest<PreparedPublicUse>('/api/vnext/pomv/public-use/prepare', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  confirmPublicUse: (intentCid: string, singleUseReceipt: string) =>
    vnextRequest<PublicationView>('/api/vnext/pomv/public-use/confirm', {
      method: 'POST',
      body: JSON.stringify({
        intent_cid: intentCid,
        single_use_receipt: singleUseReceipt,
      }),
    }),
  getPublication: (id: string) =>
    vnextRequest<PublicationView>(
      `/api/vnext/pomv/publications/${encodeURIComponent(id)}`,
    ),
  getMetabolicView: (target: string) =>
    vnextRequest<MetabolicEvidenceView>(
      `/api/vnext/pomv/views/${encodeURIComponent(target)}`,
    ),
  getVNextRuntimeStatus: () =>
    vnextRequest<VNextRuntimeStatus>('/api/vnext/runtime/status'),
};
