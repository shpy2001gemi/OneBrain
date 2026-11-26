import { getPrivateApiConnection, VNextApiError } from './client';
import type { VNextMeta } from './types';
import { logDebug } from '../components/debugLog';
import type { AuxiliarySurfaceKind } from '../utils/epistemicBadges';

// ─── Personal Mesh (`obp/personal-mesh/1`) Types ────────────────────────────

export type MeshDeviceSurfaceKind = 'desktop' | 'mobile' | 'cli' | 'web' | 'glasses';

export interface FourPrincipalSeparation {
  actor_id: string;
  device_id: string;
  feed_id: string;
  node_id: string;
  private_signing_keys_shared: false;
  feed_signer_mode: 'single_writer_per_device';
}

export interface MeshPairedDevice {
  device_id: string;
  label: string;
  surface_kind: MeshDeviceSurfaceKind;
  feed_id: string;
  node_id: string;
  hpke_pubkey_hex: string;
  mesh_epoch: number;
  status: 'active' | 'revoked';
  is_current_device: boolean;
  last_seen_unix_s: number;
  delegated_scopes: string[];
}

export interface PersonalMeshStatus {
  protocol: 'obp/personal-mesh/1';
  usability: 'USABLE_OFFLINE' | 'USABLE_WITH_OBSERVED_PEERS';
  principals: FourPrincipalSeparation;
  mesh_epoch: number;
  epoch_dek_algorithm: 'XChaCha20-Poly1305' | string;
  key_wrap_algorithm: 'X25519-HKDF-SHA256' | string;
  vault_sync_enabled: boolean;
  synced_vault_categories: string[];
  excluded_never_synced: string[];
  pending_sync_items: number;
  last_sync_unix_s: number | null;
  reachable_mesh_peers: number;
  devices: MeshPairedDevice[];
}

export interface MeshPairStartRequest {
  device_label: string;
  surface_kind: MeshDeviceSurfaceKind;
  allowed_scopes?: string[];
}

export interface MeshPairingSession {
  session_id: string;
  protocol: 'PersonalMeshPairV1';
  device_label: string;
  surface_kind: MeshDeviceSurfaceKind;
  qr_payload: string;
  sas_digits: string;
  sas_words: [string, string, string, string];
  candidate_device_id: string;
  candidate_feed_id: string;
  candidate_node_id: string;
  candidate_hpke_pubkey: string;
  target_mesh_epoch: number;
  expires_at_unix_s: number;
}

export interface MeshPairConfirmRequest {
  session_id: string;
  sas_confirmed: boolean;
  sas_digits?: string;
}

export interface ActorDelegationReceipt {
  delegation_kind: 'ActorDelegation/1' | 'ActorRootDelegation/1';
  delegation_event_cid: string;
  actor_id: string;
  new_device_id: string;
  new_feed_id: string;
  new_device_hpke_pubkey: string;
  allowed_scopes: string[];
  mesh_epoch: number;
  private_keys_transmitted: false;
  device: MeshPairedDevice;
}

export interface MeshRevokeRequest {
  device_id: string;
  reason: 'lost' | 'compromised' | 'retired';
}

export interface ActorRevocationReceipt {
  revocation_kind: 'ActorRevocation/1';
  revocation_event_cid: string;
  revoked_device_id: string;
  previous_mesh_epoch: number;
  new_mesh_epoch: number;
  epoch_dek_rotated: true;
  remaining_active_devices: number;
  historical_public_feed_verifiable: true;
}

export interface MeshSyncRequest {
  vault_sync_enabled: boolean;
  trigger_reconcile?: boolean;
}

export interface MeshSyncResult {
  mesh_epoch: number;
  vault_sync_enabled: boolean;
  disclosure_class: 'NEGOTIATED_ENCRYPTED' | 'LOCAL_ONLY';
  wrapped_recipient_devices: number;
  synced_items: number;
  last_sync_unix_s: number;
}

// ─── Scoped Surface Capability Tokens (`obc_surface1`) Types ────────────────

export interface SurfaceCapabilityTokenView {
  token_id: string;
  token_prefix: 'obc_surface1';
  token_preview: string;
  surface_kind: AuxiliarySurfaceKind;
  label: string;
  bound_channel_or_origin: string;
  granted_permissions: string[];
  hard_prohibitions: string[];
  private_vault_scope: string;
  step2_public_confirm: 'prohibited' | 'two_step_physical_and_readback_only';
  doff_lock_max_ms: number | null;
  quarantine_inbound: boolean;
  created_at_unix_s: number;
  revoked: boolean;
}

export interface MintSurfaceTokenRequest {
  surface_kind: AuxiliarySurfaceKind;
  label: string;
  bound_channel_or_origin: string;
}

export interface MintedSurfaceTokenResult {
  token: SurfaceCapabilityTokenView;
  issued_capability_handle: string;
}

// ─── Cross-Pillar Read-Only & Compiler Projection Types ─────────────────────

export interface NeedCompileRequest {
  local_query: string;
}

export interface CompiledNeedIrView {
  compiled: boolean;
  compiler_profile: 'kql-need-compiler/1';
  registry_release_root: string;
  receptor_definition_cid: string;
  query_definition_cid: string;
  selector_cid: string;
  selector_policy: string;
  statement_count: number;
  open_slot_count: number;
  constraint_count: number;
  raw_query_zeroized: true;
  limitations: string[];
}

export interface ObkgPlaneSummary {
  plane_id: number;
  name: string;
  projected_entities: string;
  node_or_edge_count: number;
  invariant: string;
}

export interface ObkgSnapshotView {
  profile: 'OBKG-002';
  disposable_projection: true;
  holds_independent_truth: false;
  snapshot_commitment_cid: string;
  source_frontier_cid: string;
  registry_release_root: string;
  coverage: 'local_only' | 'partial';
  reachable_peers: number;
  planes: ObkgPlaneSummary[];
}

export interface ObtVNextStatusView {
  profile: 'P5-OBT-ECONOMY-VNEXT-001';
  economic_status: 'simulated_non_economic';
  obt_ledger_v1_enabled: false;
  reward_evidence_export_enabled: false;
  obt_mode: 'ShadowEvaluate';
  atomic_unit_quanta_per_obt: '1000000000';
  track_a_work_claims_shadow: number;
  track_b_benefit_claims_shadow: number;
  pending_mint_records: number;
  pending_mint_spendable: false;
  settlement_state: 'PROVISIONAL_PARTITION_LOCAL';
  offline_penalty_or_slashing: false;
  limitations: string[];
}

export interface PamMemoryTierStatus {
  tier: 'M0' | 'M1' | 'M2' | 'M3';
  name: string;
  custody: string;
  retention: string;
  mesh_sync_opted_in: boolean;
}

export interface CognitionStatusView {
  profile: 'P6-AI-MEDIATOR-RUN003-001';
  usability: 'USABLE_OFFLINE' | 'USABLE_WITH_OBSERVED_PEERS';
  pam_storage: 'PrivateVault (purpose=4, XChaCha20-Poly1305, LOCAL_ONLY)';
  model_name: string;
  model_qualified: boolean;
  remote_cognition_requester: boolean;
  remote_cognition_executor: boolean;
  rag_emits_use_evidence: false;
  recommendation_guard_active: true;
  tiers: PamMemoryTierStatus[];
  limitations: string[];
}

export interface P10Envelope<T> {
  data: T;
  meta: VNextMeta;
  offlineFallback: boolean;
}

// ─── Deterministic Local Offline Fallback State (`USABLE_OFFLINE`) ──────────

const OFFLINE_META: VNextMeta = {
  lifecycle: 'disabled',
  coverage: 'local_only',
  limitations: ['usable_offline_local_fallback'],
  continuation: null,
};

const FORBIDDEN_NEED_FIELD_RE =
  /\b(pomv|trust_score|metabolic_rate|obt_balance|peer_count|wire_size|encoding_status)\b|SCOPE\s+GLOBAL/i;

function deterministicHex(seed: string, length = 64): string {
  let h1 = 0x811c9dc5;
  let h2 = 0x01000193;
  for (let i = 0; i < seed.length; i += 1) {
    const ch = seed.charCodeAt(i);
    h1 = Math.imul(h1 ^ ch, 0x01000193) >>> 0;
    h2 = Math.imul(h2 ^ ((ch << 3) | (ch >>> 5)), 0x85ebca6b) >>> 0;
  }
  let out = '';
  let round = 0;
  while (out.length < length) {
    const v1 = Math.imul(h1 ^ round, 0x27d4eb2d) >>> 0;
    const v2 = Math.imul(h2 + round * 0x9e3779b9, 0x165667b1) >>> 0;
    out += v1.toString(16).padStart(8, '0') + v2.toString(16).padStart(8, '0');
    round += 1;
  }
  return out.slice(0, length);
}

const SAS_WORD_BANK = [
  'amber', 'beacon', 'cedar', 'cobalt', 'coral', 'delta', 'ember', 'falcon',
  'fjord', 'granite', 'harbor', 'indigo', 'jasper', 'kestrel', 'lantern', 'linen',
  'meadow', 'nebula', 'onyx', 'osprey', 'prism', 'quartz', 'ripple', 'saffron',
  'solstice', 'spruce', 'summit', 'thistle', 'topaz', 'velvet', 'whisper', 'zenith',
] as const;

interface FallbackStore {
  meshEpoch: number;
  vaultSyncEnabled: boolean;
  lastSyncUnixS: number | null;
  devices: MeshPairedDevice[];
  pairingSessions: Map<string, MeshPairingSession>;
  surfaceTokens: SurfaceCapabilityTokenView[];
  seq: number;
}

function createInitialFallbackStore(): FallbackStore {
  const actorId = 'a1'.repeat(32);
  const hostDeviceId = 'b2'.repeat(32);
  const hostFeedId = 'c3'.repeat(32);
  const hostNodeId = 'd4'.repeat(32);
  const mobileDeviceId = 'e5'.repeat(32);
  const mobileFeedId = 'f6'.repeat(32);
  const mobileNodeId = '78'.repeat(32);

  return {
    meshEpoch: 1,
    vaultSyncEnabled: true,
    lastSyncUnixS: 1_764_200_000,
    seq: 1,
    pairingSessions: new Map(),
    devices: [
      {
        device_id: hostDeviceId,
        label: 'This Workstation (Primary Host)',
        surface_kind: 'desktop',
        feed_id: hostFeedId,
        node_id: hostNodeId,
        hpke_pubkey_hex: deterministicHex(`hpke:${hostDeviceId}`),
        mesh_epoch: 1,
        status: 'active',
        is_current_device: true,
        last_seen_unix_s: 1_764_200_120,
        delegated_scopes: ['private_vault_mesh', 'standing_needs', 'pam_m1_m3'],
      },
      {
        device_id: mobileDeviceId,
        label: 'Paired Sovereign Mobile',
        surface_kind: 'mobile',
        feed_id: mobileFeedId,
        node_id: mobileNodeId,
        hpke_pubkey_hex: deterministicHex(`hpke:${mobileDeviceId}`),
        mesh_epoch: 1,
        status: 'active',
        is_current_device: false,
        last_seen_unix_s: 1_764_199_800,
        delegated_scopes: ['private_vault_mesh', 'pam_m1_m3'],
      },
    ],
    surfaceTokens: [
      {
        token_id: deterministicHex(`tok:${actorId}:extension:default`, 32),
        token_prefix: 'obc_surface1',
        token_preview: 'obc_surface1_ext_9f3a…c812',
        surface_kind: 'extension',
        label: 'Browser Web Capture Adapter',
        bound_channel_or_origin: 'chrome-extension://onebrain-capture-local',
        granted_permissions: ['capture:sanitized_web_draft', 'preview:own_drafts'],
        hard_prohibitions: [
          'bulk_private_vault_read_prohibited',
          'obm1_admin_archive_prohibited',
          'step2_public_confirm_prohibited (NEG-P10-002)',
          'somatic_p9_prohibited',
        ],
        private_vault_scope: 'Own captured drafts + scoped preview only',
        step2_public_confirm: 'prohibited',
        doff_lock_max_ms: null,
        quarantine_inbound: false,
        created_at_unix_s: 1_764_190_000,
        revoked: false,
      },
      {
        token_id: deterministicHex(`tok:${actorId}:bot:default`, 32),
        token_prefix: 'obc_surface1',
        token_preview: 'obc_surface1_bot_41d0…77ea',
        surface_kind: 'bot',
        label: 'Community Matrix BotPrincipal',
        bound_channel_or_origin: 'bot-dmz://matrix.local/#science',
        granted_permissions: ['read:public_validated_store', 'submit:quarantine_record_only'],
        hard_prohibitions: [
          'owner_private_vault_zero_access (NEG-P10-003)',
          'feed_event_signer_zero_access',
          'obm1_admin_archive_prohibited',
          'step2_public_confirm_prohibited (NEG-P10-003)',
          'somatic_p9_prohibited',
        ],
        private_vault_scope: 'Zero (PUBLIC ValidatedStore only)',
        step2_public_confirm: 'prohibited',
        doff_lock_max_ms: null,
        quarantine_inbound: true,
        created_at_unix_s: 1_764_191_200,
        revoked: false,
      },
      {
        token_id: deterministicHex(`tok:${actorId}:glasses:default`, 32),
        token_prefix: 'obc_surface1',
        token_preview: 'obc_surface1_hud_6b2c…19f4',
        surface_kind: 'glasses',
        label: 'Companion Optical HUD Glasses',
        bound_channel_or_origin: 'obp/companion-hud/1://ble-mtls-companion',
        granted_permissions: [
          'read:active_hud_context_cards',
          'somatic:paired_immediate_v1',
          'confirm:two_step_physical_gesture_plus_readback',
        ],
        hard_prohibitions: [
          'zero_at_rest_master_secrets',
          'obm1_admin_archive_prohibited',
          'single_utterance_voice_confirm_prohibited',
        ],
        private_vault_scope: 'Active HUD Tier M0 context cards only (flushed <= 40 ms on doff)',
        step2_public_confirm: 'two_step_physical_and_readback_only',
        doff_lock_max_ms: 40,
        quarantine_inbound: false,
        created_at_unix_s: 1_764_192_400,
        revoked: false,
      },
    ],
  };
}

let fallbackStore: FallbackStore = createInitialFallbackStore();

/** Resets in-memory offline fallback state (used by deterministic unit/render tests). */
export function resetP10OfflineState(): void {
  fallbackStore = createInitialFallbackStore();
}

function buildFallbackMeshStatus(): PersonalMeshStatus {
  const host = fallbackStore.devices.find(d => d.is_current_device) ?? fallbackStore.devices[0];
  return {
    protocol: 'obp/personal-mesh/1',
    usability: 'USABLE_OFFLINE',
    principals: {
      actor_id: 'a1'.repeat(32),
      device_id: host.device_id,
      feed_id: host.feed_id,
      node_id: host.node_id,
      private_signing_keys_shared: false,
      feed_signer_mode: 'single_writer_per_device',
    },
    mesh_epoch: fallbackStore.meshEpoch,
    epoch_dek_algorithm: 'XChaCha20-Poly1305',
    key_wrap_algorithm: 'X25519-HKDF-SHA256',
    vault_sync_enabled: fallbackStore.vaultSyncEnabled,
    synced_vault_categories: [
      'Private KU drafts (opted-in)',
      'PersonalMeshScope KUs',
      'Private StandingNeed bundles',
      'PAM Tier M1 & Tier M3 encrypted slices',
    ],
    excluded_never_synced: [
      'FeedEventSigner private keys (NEG-P10-001)',
      'P9 SomaticBiometricRaw streams',
      'Local obm1 / obc_surface1 admin secrets',
    ],
    pending_sync_items: 0,
    last_sync_unix_s: fallbackStore.lastSyncUnixS,
    reachable_mesh_peers: 0,
    devices: fallbackStore.devices.map(d => ({ ...d })),
  };
}

function surfaceDefaults(
  surfaceKind: AuxiliarySurfaceKind,
): Pick<
  SurfaceCapabilityTokenView,
  | 'granted_permissions'
  | 'hard_prohibitions'
  | 'private_vault_scope'
  | 'step2_public_confirm'
  | 'doff_lock_max_ms'
  | 'quarantine_inbound'
> {
  switch (surfaceKind) {
    case 'extension':
      return {
        granted_permissions: ['capture:sanitized_web_draft', 'preview:own_drafts'],
        hard_prohibitions: [
          'bulk_private_vault_read_prohibited',
          'obm1_admin_archive_prohibited',
          'step2_public_confirm_prohibited (NEG-P10-002)',
          'somatic_p9_prohibited',
        ],
        private_vault_scope: 'Own captured drafts + scoped preview only',
        step2_public_confirm: 'prohibited',
        doff_lock_max_ms: null,
        quarantine_inbound: false,
      };
    case 'bot':
      return {
        granted_permissions: ['read:public_validated_store', 'submit:quarantine_record_only'],
        hard_prohibitions: [
          'owner_private_vault_zero_access (NEG-P10-003)',
          'feed_event_signer_zero_access',
          'obm1_admin_archive_prohibited',
          'step2_public_confirm_prohibited (NEG-P10-003)',
          'somatic_p9_prohibited',
        ],
        private_vault_scope: 'Zero (PUBLIC ValidatedStore only)',
        step2_public_confirm: 'prohibited',
        doff_lock_max_ms: null,
        quarantine_inbound: true,
      };
    case 'glasses':
      return {
        granted_permissions: [
          'read:active_hud_context_cards',
          'somatic:paired_immediate_v1',
          'confirm:two_step_physical_gesture_plus_readback',
        ],
        hard_prohibitions: [
          'zero_at_rest_master_secrets',
          'obm1_admin_archive_prohibited',
          'single_utterance_voice_confirm_prohibited',
        ],
        private_vault_scope: 'Active HUD Tier M0 context cards only (flushed <= 40 ms on doff)',
        step2_public_confirm: 'two_step_physical_and_readback_only',
        doff_lock_max_ms: 40,
        quarantine_inbound: false,
      };
  }
}

function redactDebugBody(path: string, body: string | undefined): string | undefined {
  if (!body) return undefined;
  if (path === '/api/vnext/kql/needs/compile') {
    try {
      const parsed = JSON.parse(body) as Record<string, unknown>;
      parsed.local_query = '[PRIVATE_REDACTED]';
      return JSON.stringify(parsed);
    } catch {
      return '[PRIVATE_REDACTED]';
    }
  }
  return body;
}

type RawVNextEnvelope<T> =
  | { ok: true; profile?: string; data: T; meta?: VNextMeta }
  | {
      ok: false;
      profile?: string;
      error: {
        code: string;
        message: string;
        retryable?: boolean;
        limitations?: string[];
      };
      meta?: VNextMeta;
    };

async function p10RequestWithFallback<T>(
  path: string,
  opts: RequestInit,
  buildFallback: () => T,
): Promise<P10Envelope<T>> {
  const method = (opts.method || 'GET').toUpperCase();
  const rawBody = opts.body ? String(opts.body) : undefined;
  logDebug({ type: 'request', method, path, body: redactDebugBody(path, rawBody) });

  try {
    const conn = await getPrivateApiConnection();
    const res = await fetch(`${conn.baseUrl}${path}`, {
      ...opts,
      headers: {
        'Content-Type': 'application/json',
        Authorization: `Bearer ${conn.token}`,
        ...opts.headers,
      },
    });

    if (res.status === 404 || res.status === 501 || res.status === 502 || res.status === 503) {
      return {
        data: buildFallback(),
        meta: OFFLINE_META,
        offlineFallback: true,
      };
    }

    const json = (await res.json()) as RawVNextEnvelope<T>;
    if (!json.ok) {
      const meta = json.meta ?? OFFLINE_META;
      throw new VNextApiError(
        json.error.message || json.error.code,
        json.error.code,
        meta,
        Boolean(json.error.retryable),
      );
    }

    return {
      data: json.data,
      meta: json.meta ?? {
        lifecycle: 'active',
        coverage: 'local_only',
        limitations: [],
        continuation: null,
      },
      offlineFallback: false,
    };
  } catch (err) {
    if (err instanceof VNextApiError) {
      throw err;
    }
    return {
      data: buildFallback(),
      meta: OFFLINE_META,
      offlineFallback: true,
    };
  }
}

// ─── Exported Typed P10 Client ──────────────────────────────────────────────

export const p10Api = {
  getMeshStatus: (): Promise<P10Envelope<PersonalMeshStatus>> =>
    p10RequestWithFallback('/api/vnext/mesh/status', { method: 'GET' }, buildFallbackMeshStatus),

  startMeshPairing: (
    input: MeshPairStartRequest,
  ): Promise<P10Envelope<MeshPairingSession>> => {
    const label = input.device_label.trim() || `Paired ${input.surface_kind}`;
    return p10RequestWithFallback(
      '/api/vnext/mesh/pair/start',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
      () => {
        const idx = fallbackStore.seq++;
        const sessionId = deterministicHex(`pair-session:${idx}:${label}`, 32);
        const candidateDeviceId = deterministicHex(`device:${idx}:${label}`);
        const candidateFeedId = deterministicHex(`feed:${idx}:${label}`);
        const candidateNodeId = deterministicHex(`node:${idx}:${label}`);
        const candidateHpke = deterministicHex(`hpke:${idx}:${label}`);
        const num = (parseInt(sessionId.slice(0, 6), 16) % 900_000) + 100_000;
        const sasDigits = `${String(num).slice(0, 3)}-${String(num).slice(3, 6)}`;
        const w = (offset: number) =>
          SAS_WORD_BANK[parseInt(sessionId.slice(offset, offset + 4), 16) % SAS_WORD_BANK.length];
        const session: MeshPairingSession = {
          session_id: sessionId,
          protocol: 'PersonalMeshPairV1',
          device_label: label,
          surface_kind: input.surface_kind,
          qr_payload: `obp://personal-mesh/pair/1?sid=${sessionId.slice(0, 16)}&spake2=${deterministicHex(`spake2:${sessionId}`, 32)}&epoch=${fallbackStore.meshEpoch}`,
          sas_digits: sasDigits,
          sas_words: [w(0), w(4), w(8), w(12)],
          candidate_device_id: candidateDeviceId,
          candidate_feed_id: candidateFeedId,
          candidate_node_id: candidateNodeId,
          candidate_hpke_pubkey: candidateHpke,
          target_mesh_epoch: fallbackStore.meshEpoch,
          expires_at_unix_s: Math.floor(Date.now() / 1000) + 300,
        };
        fallbackStore.pairingSessions.set(sessionId, session);
        return session;
      },
    );
  },

  confirmMeshPairing: async (
    input: MeshPairConfirmRequest,
  ): Promise<P10Envelope<ActorDelegationReceipt>> => {
    if (!input.sas_confirmed) {
      throw new Error(
        'SAS_CONFIRMATION_REQUIRED: Explicit user verification of the 6-digit and 4-word SAS is required before issuing ActorDelegation/1.',
      );
    }
    return p10RequestWithFallback(
      '/api/vnext/mesh/pair/confirm',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
      () => {
        const session = fallbackStore.pairingSessions.get(input.session_id);
        const deviceId =
          session?.candidate_device_id ?? deterministicHex(`confirmed-dev:${input.session_id}`);
        const feedId =
          session?.candidate_feed_id ?? deterministicHex(`confirmed-feed:${input.session_id}`);
        const nodeId =
          session?.candidate_node_id ?? deterministicHex(`confirmed-node:${input.session_id}`);
        const hpke =
          session?.candidate_hpke_pubkey ?? deterministicHex(`confirmed-hpke:${input.session_id}`);
        const scopes = ['private_vault_mesh', 'standing_needs', 'pam_m1_m3'];
        const newDevice: MeshPairedDevice = {
          device_id: deviceId,
          label: session?.device_label ?? 'Newly Enrolled Mesh Device',
          surface_kind: session?.surface_kind ?? 'mobile',
          feed_id: feedId,
          node_id: nodeId,
          hpke_pubkey_hex: hpke,
          mesh_epoch: fallbackStore.meshEpoch,
          status: 'active',
          is_current_device: false,
          last_seen_unix_s: Math.floor(Date.now() / 1000),
          delegated_scopes: scopes,
        };
        fallbackStore.pairingSessions.delete(input.session_id);
        fallbackStore.devices = [
          ...fallbackStore.devices.filter(d => d.device_id !== deviceId),
          newDevice,
        ];
        return {
          delegation_kind: 'ActorDelegation/1',
          delegation_event_cid: deterministicHex(`delegation:${input.session_id}:${fallbackStore.meshEpoch}`),
          actor_id: 'a1'.repeat(32),
          new_device_id: deviceId,
          new_feed_id: feedId,
          new_device_hpke_pubkey: hpke,
          allowed_scopes: scopes,
          mesh_epoch: fallbackStore.meshEpoch,
          private_keys_transmitted: false,
          device: newDevice,
        };
      },
    );
  },

  revokeMeshDevice: (
    input: MeshRevokeRequest,
  ): Promise<P10Envelope<ActorRevocationReceipt>> =>
    p10RequestWithFallback(
      '/api/vnext/mesh/revoke',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
      () => {
        const previousEpoch = fallbackStore.meshEpoch;
        const nextEpoch = previousEpoch + 1;
        fallbackStore.meshEpoch = nextEpoch;
        fallbackStore.devices = fallbackStore.devices.map(dev => {
          if (dev.device_id === input.device_id && !dev.is_current_device) {
            return { ...dev, status: 'revoked' as const };
          }
          if (dev.status === 'active') {
            return { ...dev, mesh_epoch: nextEpoch };
          }
          return dev;
        });
        const remainingActive = fallbackStore.devices.filter(d => d.status === 'active').length;
        return {
          revocation_kind: 'ActorRevocation/1',
          revocation_event_cid: deterministicHex(`revoke:${input.device_id}:${nextEpoch}`),
          revoked_device_id: input.device_id,
          previous_mesh_epoch: previousEpoch,
          new_mesh_epoch: nextEpoch,
          epoch_dek_rotated: true,
          remaining_active_devices: remainingActive,
          historical_public_feed_verifiable: true,
        };
      },
    ),

  syncPersonalMesh: (
    input: MeshSyncRequest,
  ): Promise<P10Envelope<MeshSyncResult>> =>
    p10RequestWithFallback(
      '/api/vnext/mesh/sync',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
      () => {
        fallbackStore.vaultSyncEnabled = input.vault_sync_enabled;
        const now = Math.floor(Date.now() / 1000);
        if (input.vault_sync_enabled) {
          fallbackStore.lastSyncUnixS = now;
        }
        const activeCount = fallbackStore.devices.filter(d => d.status === 'active').length;
        return {
          mesh_epoch: fallbackStore.meshEpoch,
          vault_sync_enabled: fallbackStore.vaultSyncEnabled,
          disclosure_class: fallbackStore.vaultSyncEnabled ? 'NEGOTIATED_ENCRYPTED' : 'LOCAL_ONLY',
          wrapped_recipient_devices: fallbackStore.vaultSyncEnabled ? activeCount : 0,
          synced_items: fallbackStore.vaultSyncEnabled ? 4 : 0,
          last_sync_unix_s: fallbackStore.lastSyncUnixS ?? now,
        };
      },
    ),

  listSurfaceTokens: (): Promise<P10Envelope<{ items: SurfaceCapabilityTokenView[] }>> =>
    p10RequestWithFallback(
      '/api/vnext/surfaces/tokens',
      { method: 'GET' },
      () => ({
        items: fallbackStore.surfaceTokens.map(t => ({ ...t })),
      }),
    ),

  mintSurfaceToken: (
    input: MintSurfaceTokenRequest,
  ): Promise<P10Envelope<MintedSurfaceTokenResult>> =>
    p10RequestWithFallback(
      '/api/vnext/surfaces/tokens',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
      () => {
        const idx = fallbackStore.seq++;
        const tokenId = deterministicHex(`mint-surface:${idx}:${input.surface_kind}:${input.label}`, 32);
        const shortHash = tokenId.slice(0, 4);
        const tailHash = tokenId.slice(-4);
        const defaults = surfaceDefaults(input.surface_kind);
        const token: SurfaceCapabilityTokenView = {
          token_id: tokenId,
          token_prefix: 'obc_surface1',
          token_preview: `obc_surface1_${input.surface_kind.slice(0, 3)}_${shortHash}…${tailHash}`,
          surface_kind: input.surface_kind,
          label: input.label.trim() || `${input.surface_kind} adapter`,
          bound_channel_or_origin:
            input.bound_channel_or_origin.trim() || `loopback://${input.surface_kind}`,
          ...defaults,
          created_at_unix_s: Math.floor(Date.now() / 1000),
          revoked: false,
        };
        fallbackStore.surfaceTokens = [token, ...fallbackStore.surfaceTokens];
        return {
          token,
          issued_capability_handle: `obc_surface1.${input.surface_kind}.${tokenId.slice(0, 16)}`,
        };
      },
    ),

  revokeSurfaceToken: (
    tokenId: string,
  ): Promise<P10Envelope<{ token_id: string; revoked: true }>> =>
    p10RequestWithFallback(
      `/api/vnext/surfaces/tokens/${encodeURIComponent(tokenId)}`,
      { method: 'DELETE' },
      () => {
        fallbackStore.surfaceTokens = fallbackStore.surfaceTokens.map(tok =>
          tok.token_id === tokenId ? { ...tok, revoked: true } : tok,
        );
        return { token_id: tokenId, revoked: true };
      },
    ),

  compileNeedIr: async (
    input: NeedCompileRequest,
  ): Promise<P10Envelope<CompiledNeedIrView>> => {
    const trimmed = input.local_query.trim();
    if (FORBIDDEN_NEED_FIELD_RE.test(trimmed)) {
      throw new VNextApiError(
        'Non-semantic runtime, legacy, or economic fields are forbidden in deterministic NeedIR compilation.',
        'non_semantic_field_forbidden_in_need_ir',
        OFFLINE_META,
        false,
      );
    }
    const normalized = trimmed.replace(/\s+/g, ' ').replace(/\?[a-zA-Z0-9_]+/g, '?slot_0');
    return p10RequestWithFallback(
      '/api/vnext/kql/needs/compile',
      {
        method: 'POST',
        body: JSON.stringify(input),
      },
      () => ({
        compiled: true,
        compiler_profile: 'kql-need-compiler/1',
        registry_release_root: '88'.repeat(32),
        receptor_definition_cid: deterministicHex(`receptor:${normalized}`),
        query_definition_cid: deterministicHex(`query-def:${normalized}`),
        selector_cid: deterministicHex('policy:public-knowledge-exchange-v1'),
        selector_policy: 'public-knowledge-exchange-v1 (PolicyRegistry allow-listed)',
        statement_count: 1,
        open_slot_count: normalized.includes('?slot_0') ? 1 : 0,
        constraint_count: /\bWHERE\b/i.test(trimmed) ? 1 : 0,
        raw_query_zeroized: true,
        limitations: [],
      }),
    );
  },

  getObkgSnapshot: (): Promise<P10Envelope<ObkgSnapshotView>> =>
    p10RequestWithFallback(
      '/api/vnext/obkg/snapshot',
      { method: 'GET' },
      () => ({
        profile: 'OBKG-002',
        disposable_projection: true,
        holds_independent_truth: false,
        snapshot_commitment_cid: deterministicHex('obkg-snapshot-v1'),
        source_frontier_cid: deterministicHex('obkg-source-frontier-v1'),
        registry_release_root: '88'.repeat(32),
        coverage: 'local_only',
        reachable_peers: 0,
        planes: [
          {
            plane_id: 1,
            name: 'Semantic Statement Plane',
            projected_entities: 'StatementFrame hyperedges & ConceptCcid nodes',
            node_or_edge_count: 0,
            invariant: 'Marks [retired_concept] & redirects relative to RegistryReleaseRoot without rewriting P1 bytes.',
          },
          {
            plane_id: 2,
            name: 'Gap & Affordance Plane',
            projected_entities: 'FrontierAssemblyManifest hypernodes & Receptor/Affordance ports',
            node_or_edge_count: 0,
            invariant: 'Preserves 3-state (Satisfied / Violated / Unknown) constraint annotations.',
          },
          {
            plane_id: 3,
            name: 'Three-Lane Mapping & Resolution Plane',
            projected_entities: 'QuarantinedProposal, MaterializedInactive, AdoptedActiveRelative edges',
            node_or_edge_count: 0,
            invariant: 'Active edge iff materialized (KU-006) and adopted (KU-007) at snapshot frontier.',
          },
          {
            plane_id: 4,
            name: 'Evidence & Fidelity Overlay Plane',
            projected_entities: 'MetabolicEvidenceView, OutcomeBranchView & FidelityAssessment',
            node_or_edge_count: 0,
            invariant: 'Preserves zero-use, disputed, and refuted subgraphs (NEG-KU-002).',
          },
          {
            plane_id: 5,
            name: 'Frontier-Anchored Historical Plane',
            projected_entities: 'AT FRONTIER & DIFF FRONTIERS cryptographic checkpoints',
            node_or_edge_count: 1,
            invariant: 'Anchored to FeedCheckpointCid (CHK-001), never unverified wall-clock timestamps.',
          },
        ],
      }),
    ),

  getObtStatus: (): Promise<P10Envelope<ObtVNextStatusView>> =>
    p10RequestWithFallback(
      '/api/vnext/obt/status',
      { method: 'GET' },
      () => ({
        profile: 'P5-OBT-ECONOMY-VNEXT-001',
        economic_status: 'simulated_non_economic',
        obt_ledger_v1_enabled: false,
        reward_evidence_export_enabled: false,
        obt_mode: 'ShadowEvaluate',
        atomic_unit_quanta_per_obt: '1000000000',
        track_a_work_claims_shadow: 0,
        track_b_benefit_claims_shadow: 0,
        pending_mint_records: 0,
        pending_mint_spendable: false,
        settlement_state: 'PROVISIONAL_PARTITION_LOCAL',
        offline_penalty_or_slashing: false,
        limitations: [
          'obt_ledger_v1_disabled_until_m7_gate',
          'shadow_evaluate_zero_account_block_mutation',
        ],
      }),
    ),

  getCognitionStatus: (): Promise<P10Envelope<CognitionStatusView>> =>
    p10RequestWithFallback(
      '/api/vnext/cognition/status',
      { method: 'GET' },
      () => ({
        profile: 'P6-AI-MEDIATOR-RUN003-001',
        usability: 'USABLE_OFFLINE',
        pam_storage: 'PrivateVault (purpose=4, XChaCha20-Poly1305, LOCAL_ONLY)',
        model_name: 'local-companion (unqualified)',
        model_qualified: false,
        remote_cognition_requester: false,
        remote_cognition_executor: false,
        rag_emits_use_evidence: false,
        recommendation_guard_active: true,
        tiers: [
          {
            tier: 'M0',
            name: 'EphemeralTurnBuffer',
            custody: 'In-RAM mlock/zeroize buffer only',
            retention: 'Wiped immediately on turn completion or abort',
            mesh_sync_opted_in: false,
          },
          {
            tier: 'M1',
            name: 'WorkingSessionThread',
            custody: 'PrivateVault (purpose=4, LOCAL_ONLY)',
            retention: 'Bounded rolling window (<= 256 turns)',
            mesh_sync_opted_in: fallbackStore.vaultSyncEnabled,
          },
          {
            tier: 'M2',
            name: 'EpisodicObservationLinks',
            custody: 'AI-003 ObservationEvent + SourceArtifact refs',
            retention: 'Immutable provenance pointers to private drafts',
            mesh_sync_opted_in: false,
          },
          {
            tier: 'M3',
            name: 'SovereignActorPreferences',
            custody: 'PrivateVault (purpose=4, LOCAL_ONLY)',
            retention: 'User-inspected vocabulary & PrivateOnly defaults; never sent to RUN-003 workers',
            mesh_sync_opted_in: fallbackStore.vaultSyncEnabled,
          },
        ],
        limitations: ['model_qualified_false', 'remote_cognition_disabled_by_default'],
      }),
    ),
};
