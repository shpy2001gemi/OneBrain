import type {
  ApiResponse, StatusResponse, IdentityInfo, KuListResponse,
  KuDetail, EncodeResult, ChatResponse, WalletInfo,
  WalletTransaction, UserProfile, ConfigView, ModelInfo,
  AiHealthInfo, PeerView,
} from './types';
import { logDebug } from '../components/DebugConsole';

const API_BASE = 'http://127.0.0.1:4280';

function getToken(): string {
  return localStorage.getItem('ob_api_token') || '';
}

export function setToken(token: string) {
  localStorage.setItem('ob_api_token', token);
}

// Used by future logout flow
export function clearToken() {
  localStorage.removeItem('ob_api_token');
}

async function request<T>(path: string, opts: RequestInit = {}): Promise<T> {
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
  } catch (err: any) {
    const duration = Math.round(performance.now() - start);
    if (err.message && !err.message.includes('AUTH_')) {
      logDebug({
        type: 'error', method, path, duration,
        message: err.message,
      });
    }
    throw err;
  }
}

// ─── Identity ────────────────────
export const api = {
  getIdentity: () => request<IdentityInfo>('/api/identity'),
  recoverIdentity: (phrase: string[], password: string) =>
    request<IdentityInfo>('/api/identity/recover', {
      method: 'POST', body: JSON.stringify({ recovery_phrase: phrase, new_password: password }),
    }),

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
    request<unknown>('/api/search', {
      method: 'POST', body: JSON.stringify({ query, limit }),
    }),
  kql: (query: string) =>
    request<{ results: unknown[] }>('/api/kql', {
      method: 'POST', body: JSON.stringify({ query }),
    }),

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
    request<unknown>(`/api/graph/${cid}?depth=${depth}`),
  getNeighbors: (cid: string) =>
    request<unknown>(`/api/graph/${cid}/neighbors`),

  // ─── Wallet ─────────────────────
  getWallet: () => request<WalletInfo>('/api/wallet'),
  getWalletHistory: (limit = 50) =>
    request<WalletTransaction[]>(`/api/wallet/history?limit=${limit}`),

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
  listModels: () => request<ModelInfo[]>('/api/ai/models'),
  switchModel: (modelName: string) =>
    request<{ ok: boolean }>('/api/ai/model', {
      method: 'POST', body: JSON.stringify({ model_name: modelName }),
    }),
};
