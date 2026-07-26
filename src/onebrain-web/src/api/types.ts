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

// ─── Gene Types (KU v7) ─────────────────────────────────
// Canonical list matching ku-core::types::GeneType enum order.
// All web-based platforms (web, extension, desktop-web) should
// import these constants instead of maintaining their own lists.

export type GeneType =
  | 'Fact' | 'Procedure' | 'Experience' | 'Creative'
  | 'MediaExperience' | 'Testimony' | 'Formal' | 'Hypothesis'
  | 'Narrative' | 'Sensory' | 'Composite'
  | 'Normative' | 'Definition';

export const ALL_GENE_TYPES: GeneType[] = [
  'Fact', 'Procedure', 'Experience', 'Creative',
  'MediaExperience', 'Testimony', 'Formal', 'Hypothesis',
  'Narrative', 'Sensory', 'Composite', 'Normative', 'Definition',
];

/** Suggested UI colors for each gene type (dark-theme friendly). */
export const GENE_TYPE_COLORS: Record<GeneType, string> = {
  Fact: '#06b6d4',
  Procedure: '#8b5cf6',
  Experience: '#f59e0b',
  Creative: '#10b981',
  MediaExperience: '#ec4899',
  Testimony: '#f97316',
  Formal: '#6366f1',
  Hypothesis: '#14b8a6',
  Narrative: '#a855f7',
  Sensory: '#eab308',
  Composite: '#64748b',
  Normative: '#ef4444',
  Definition: '#0ea5e9',
};

// ─── Identity ────────────────────────────────────────────


// ─── Knowledge ───────────────────────────────────────────

export interface KuListItem {
  cid_hex: string;
  gene_type: GeneType;
  preview: string;
  pomv: number;
  pomv_profile: 'legacy_local_pomv_scalar_v1';
  pomv_is_economic: false;
  trust: number;
  created: number;
  wire_size: number;
}

export interface KuDetail {
  cid_hex: string;
  gene_type: GeneType;
  content: string;
  codons: CodonView[];
  bonds: BondView[];
  trust: number;
  pomv: number;
  pomv_profile: 'legacy_local_pomv_scalar_v1';
  pomv_is_economic: false;
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
  decoded_instructions: InstructionView[];
}

export interface CodonView {
  name: string;
  role: string;
}

export interface InstructionView {
  op: string;
  description: string;
  concept_ids: number[];
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
  gene_type: GeneType | null;
  confidence: number;
  source_text: string;
  peers_reached?: number;
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
  /** Number of KUs auto-encoded from this message */
  kus_encoded?: number;
  /** Number of KUs retrieved/referenced in this response */
  kus_retrieved?: number;
  /** AI intent classification */
  intent?: string | null;
}

// ─── Network ─────────────────────────────────────────────

export interface StatusResponse {
  ku_count: number;
  peer_count: number;
  uptime_s: number;
  node_name: string;
  tier: string;
  obt_balance: number;
  obt_economic_status: 'simulated_non_economic';
  version: string;
  model: string;
  concept_registry: ConceptRegistryStatus;
  vnext: VNextStatusSnapshot;
}

export interface ConceptRegistryStatus {
  mode: 'required' | 'optional' | 'disabled';
  state: 'LOADED' | 'FALLBACK_V1' | 'DISABLED';
  path: string;
  encoder_version: number;
  backend: 'IN_MEMORY' | 'INDEXED_ON_DEMAND' | null;
  cache_capacity: number;
  obr_schema_version: number | null;
  manifest_version: number | null;
  concept_count: number | null;
  label_count: number | null;
  checksum_blake3: string | null;
  built_at_utc: string | null;
  builder_version: string | null;
  source_snapshots: Record<string, string>;
  failure_kind: 'MISSING' | 'CORRUPT' | 'TRUNCATED' | 'UNSUPPORTED' | 'RESOURCE_LIMIT' | 'MANIFEST' | 'IO' | null;
  error?: string;
}

export interface VNextStatusSnapshot {
  profile_major: number;
  usability: 'USABLE_OFFLINE' | 'USABLE_WITH_OBSERVED_PEERS';
  reachability: {
    scope: 'LOCAL_NODE' | 'OBSERVED_PEER_SET';
    observed_peer_count: number;
    standalone: boolean;
    claims_network_completion: false;
  };
  coverage: {
    status: 'LOCAL_ONLY' | 'PARTIAL';
    local_record_count: number;
    assessed_frontier: number[] | null;
    limitations: string[];
  };
  fidelity: {
    status: 'UNASSESSED' | 'SELF_ATTESTED' | 'PARTIALLY_CORROBORATED' | 'CORROBORATED_RELATIVE_TO_FRONTIER';
    assessed_frontier: number[] | null;
    limitations: string[];
    establishes_proposition_truth: false;
  };
  legacy: {
    raw_v1_readable: boolean;
    adapter_active: boolean;
    normalized_claims_are_advisory: true;
    warnings: string[];
  };
  consent: {
    continuous_local_observation: ConsentView;
    knowledge_publish: ConsentView;
    public_need_disclosure: ConsentView;
    remote_cognition: ConsentView;
    consent_is_inferred: false;
  };
  features: {
    object_event_v1_requested: boolean;
    obp_rp_requested: boolean;
    object_event_v1: boolean;
    obp_rp: boolean;
    provider_lease: boolean;
    fidelity: boolean;
    checkpoint_gc: boolean;
    legacy_adapter: boolean;
  };
  network_runtime: {
    compiled: boolean;
    lifecycle: 'DISABLED' | 'BUILD_UNAVAILABLE' | 'CONFIGURED' | 'LISTENING';
    listen_addr: string | null;
    authenticated_sessions: number;
    active_sessions: number;
    accepted_records: number;
    deferred_records: number;
    rejected_records: number;
    claims_network_completion: false;
  };
}

export type ConsentView =
  | 'NOT_CONFIGURED'
  | 'NOT_GRANTED'
  | 'EXPLICIT_ACTION_REQUIRED'
  | 'GRANTED_LOCAL_ONLY'
  | 'GRANTED_FOR_NAMED_SCOPE';

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
  gene_type: GeneType;
  pomv: number;
  is_local: boolean;
  children: NeighborInfo[];
}

// ─── Wallet ──────────────────────────────────────────────

export interface WalletInfo {
  economic_status: 'simulated_non_economic';
  limitations: string[];
  balance: number;
  chain_length: number;
  tier: string;
  multiplier: number;
  total_earned: number;
  total_spent: number;
  staked: number;
  pending_unstake: number;
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
  economic_status: 'simulated_non_economic';
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

// ─── Phase 1: New Types ─────────────────────────────────

export interface BlobMeta {
  blob_cid_hex: string;
  original_name: string;
  mime_type: string;
  total_size: number;
  chunk_count: number;
}

export interface FollowedNode {
  node_id: string;
  name: string;
  followed_at: number;
}

export interface PeerProfile {
  node_id: string;
  name: string;
  trust_score: number;
  tier: string;
  ku_count: number;
  expertise: string[];
  member_since: number;
}

export interface DeviceInfo {
  device_id: string;
  name: string;
  device_type: string;
  last_seen: number;
  ku_count: number;
  sync_status: string;
}

export interface SyncStatus {
  status: string;
  pending_count: number;
  last_sync: number;
  devices: DeviceInfo[];
}

export interface WatchInfo {
  watch_id: string;
  query: string;
  created: number;
  match_count: number;
}

export interface ImportResult {
  imported: number;
  skipped: number;
  errors: number;
}

export interface BulkDeleteResult {
  deleted: number;
  skipped: number;
}


export interface Draft {
  id: string;
  title: string;
  text: string;
  created: number;
  updated: number;
}

// ─── vNext product surface ──────────────────────────────────────────────────

export type VNextLifecycle = 'disabled' | 'requested' | 'active' | 'degraded';
export type VNextCoverage = 'local_only' | 'partial';

export interface VNextMeta {
  lifecycle: VNextLifecycle;
  coverage: VNextCoverage;
  limitations: string[];
  continuation: string | null;
}

export interface VNextResult<T> {
  data: T;
  meta: VNextMeta;
}

export interface VNextScope {
  kind: 'one_hop' | 'authenticated_direct_peers';
  max_hops: number;
  node_ids: string[];
}

export interface VNextBudget {
  max_scan_records: number;
  max_affordances: number;
  max_pairs: number;
  max_proposals: number;
}

export interface NeedPrepareRequest {
  local_query: string;
  scope: VNextScope;
  budget: VNextBudget;
  idempotency_key: string;
}

export interface PreparedNeed {
  intent_cid: string;
  query_definition_cid: string;
  selector_cid: string;
  scope: VNextScope;
  budget: VNextBudget;
  expires_at: number;
  limitations: string[];
}

export interface NeedView {
  standing_need_id: string;
  state: string;
  query_definition_cid: string;
  selector_cid: string;
  coverage: VNextCoverage;
  limitations: string[];
  revision: number;
}

export interface NeedPage {
  items: NeedView[];
  coverage: VNextCoverage;
  limitations: string[];
  continuation: string | null;
}

export interface ConstraintObservation {
  constraint_index: number;
  evaluation: string;
  required: boolean;
}

export interface QuarantinedMatch {
  proposal_cid: string;
  candidate_cid: string;
  responder_scope: VNextScope;
  selector_cid: string;
  assessed_frontier: string;
  constraints: {
    observations: ConstraintObservation[];
    all_required_satisfied: boolean;
  };
  limitations: string[];
  state: 'quarantined';
  executable: false;
}

export interface MatchPage {
  items: QuarantinedMatch[];
  coverage: VNextCoverage;
  limitations: string[];
  continuation: string | null;
}

export type VNextUseMode =
  | 'application'
  | 'transformation'
  | 'epistemic'
  | 'transfer'
  | 'discovery'
  | 'receptor_discovered'
  | 'candidate_evaluated'
  | 'constraint_clarified'
  | 'gap_partially_filled'
  | 'assembly_used'
  | 'analogical_transfer'
  | 'compared_or_opposed'
  | 'capability_result_used';

export interface PublicUsePrepareRequest {
  target_cid: string;
  recipient_node_id: string;
  selector_cid: string;
  namespace: string;
  disclosure: {
    classification: 'public';
    permanent: true;
    use_mode: VNextUseMode;
  };
  idempotency_key: string;
  expires_at: number;
}

export interface PreparedPublicUse {
  intent_cid: string;
  canonical_payload_preview: string;
  exact_target: string;
  exact_recipient: string;
  selector_cid: string;
  namespace: string;
  disclosure: PublicUsePrepareRequest['disclosure'];
  idempotency_key: string;
  expires_at: number;
}

export interface PublicationView {
  publication_cid: string;
  intent_cid: string;
  state: 'pending' | 'deferred' | 'delivered' | string;
  attempts: number;
  limitations: string[];
  revision: number;
}

export interface MetabolicEvidenceView {
  target_cid: string;
  policy_cid: string;
  assessed_frontier: string;
  revision: number;
  use_event_root: string;
  conflicts: string[];
  coverage: VNextCoverage;
  limitations: string[];
  establishes_truth: false;
  establishes_benefit: false;
  authorizes_reward: false;
  claims_global_completion: false;
}

export interface VNextRuntimeStatus {
  compiled: boolean;
  requested: boolean;
  active: boolean;
  kill_switch: boolean;
  signer_ready: boolean;
  lifecycle: VNextLifecycle;
  coverage: VNextCoverage;
  limitations: string[];
}
