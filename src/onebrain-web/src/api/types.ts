// ─── API Envelope ────────────────────────────────────────

export interface ApiSuccess<T> {
  ok: true;
  data: T;
}

export interface ApiError {
  ok: false;
  error: {
    code: string;
    message: string;
    details?: unknown;
  };
}

export type ApiResponse<T> = ApiSuccess<T> | ApiError;

// ─── Identity ────────────────────────────────────────────

export interface IdentityInfo {
  node_id: string;
  name: string;
  created: number;
  tier: string;
  trust_score: number;
  device_count: number;
  max_devices: number;
  kus_encoded: number;
  kus_received: number;
  total_queries: number;
}

// ─── Knowledge ───────────────────────────────────────────

export interface KuListItem {
  cid_hex: string;
  gene_type: string;
  preview: string;
  pomv: number;
  trust: number;
  created: number;
  wire_size: number;
}

export interface KuDetail {
  cid_hex: string;
  gene_type: string;
  content: string;
  codons: CodonView[];
  bonds: BondView[];
  trust: number;
  pomv: number;
  pomv_breakdown: PomvBreakdown;
  epistemic: string;
  evidence: string;
  wire_size: number;
  instruction_count: number;
  confidence: number;
  created: number;
  verification_status: string;
  outgoing_bond_count: number;
  incoming_bond_count: number;
}

export interface CodonView {
  name: string;
  role: string;
}

export interface BondView {
  direction: string;
  relation: string;
  other_cid: string;
  other_preview: string;
  weight: number;
}

export interface PomvBreakdown {
  metabolic: number;
  prediction: number;
  entropy: number;
  survival: number;
  centrality: number;
  niche: number;
}

export interface EncodeResult {
  cid_hex: string;
  wire_size: number;
  instruction_count: number;
  gene_type: string | null;
  confidence: number;
  source_text: string;
}

export interface KuListResponse {
  kus: KuListItem[];
  total: number;
  page: number;
}

// ─── Chat ────────────────────────────────────────────────

export interface ChatResponse {
  text: string;
  intent: string | null;
  suggestions: string[];
  kus_encoded: number;
  kus_retrieved: number;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
}

// ─── Network ─────────────────────────────────────────────

export interface StatusResponse {
  ku_count: number;
  peer_count: number;
  uptime_s: number;
  node_name: string;
  tier: string;
  obt_balance: number;
  version: string;
}

export interface PeerView {
  name: string;
  addr: string;
  ku_count: number;
}

// ─── Graph ───────────────────────────────────────────────

export interface NeighborInfo {
  cid_hex: string;
  relation: string;
  direction: string;
  preview: string;
  weight: number;
  gene_type: string;
  pomv: number;
  is_local: boolean;
  children: NeighborInfo[];
}

// ─── Wallet ──────────────────────────────────────────────

export interface WalletInfo {
  balance: number;
  chain_length: number;
  tier: string;
  multiplier: number;
  total_earned: number;
  total_spent: number;
  streams: EarningsStreams;
  rate_used: number;
  rate_max: number;
}

export interface EarningsStreams {
  owner: number;
  encoder: number;
  verifier: number;
  storage: number;
}

export interface WalletTransaction {
  block_type: string;
  amount: number;
  detail: string;
  timestamp: number;
  confirmation: string;
}

// ─── Profile & Settings ──────────────────────────────────

export interface UserProfile {
  name: string;
  language: string;
  style: string;
  expertise: { domain: string; ku_count: number; last_active: number }[];
  total_kus: number;
  total_queries: number;
  member_since: number;
}

export interface ConfigView {
  name: string;
  port: number;
  data_dir: string;
  ollama_url: string;
  model: string;
  seeds: string[];
  identity_path: string;
  storage_path: string;
  profile_path: string;
  peers_path: string;
}

// ─── AI ──────────────────────────────────────────────────

export interface ModelInfo {
  name: string;
  params: string;
  is_current: boolean;
  is_installed: boolean;
}

export interface AiHealthInfo {
  connected: boolean;
  model: string;
  ollama_url: string;
  latency_ms: number;
  status_message: string;
}

// ─── WebSocket ───────────────────────────────────────────

export interface WsEvent {
  event_type: string;
  timestamp: number;
  data: Record<string, unknown>;
}
