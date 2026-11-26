/**
 * Canonical Pillar 10 (`P10-SURFACES-SYNC-001` §2.2) epistemic, reachability,
 * economic compatibility, and `obc_surface1` capability ceiling formatters.
 *
 * Hard invariant (`RUN-004` / `P0-ADR-001`): surfaces MUST NEVER emit
 * `FULL`, `GLOBAL`, or `CLOSED` completeness claims or aliases.
 */

export const PROVENANCE_PRIVATE_LOCAL_ONLY = '[Private · Local Only]' as const;
export const PROVENANCE_PERSONAL_MESH_ENCRYPTED = '[Personal Mesh · Encrypted]' as const;
export const PROVENANCE_PUBLIC_UNASSESSED = '[Public · Unassessed]' as const;

export const COVERAGE_LOCAL_STORE_ONLY = '[Local Store Only]' as const;

export const ECONOMIC_COMPATIBILITY_BADGE =
  '[Simulated · Non-Economic Compatibility View]' as const;

export const PROVISIONAL_PARTITION_SETTLEMENT_BADGE =
  '[Provisional — unquorumed partition state, subject to double-spend fork risk]' as const;

export const MODEL_UNASSESSED_BADGE = '[Unassessed]' as const;
export const USABLE_OFFLINE_BADGE = 'USABLE_OFFLINE' as const;

export const FORBIDDEN_COMPLETENESS_ALIASES = ['FULL', 'GLOBAL', 'CLOSED'] as const;

const FORBIDDEN_COMPLETENESS_RE = /\b(FULL|GLOBAL|CLOSED)\b/i;

/** Returns true if a label or UI string contains a forbidden completeness alias. */
export function containsForbiddenCompletenessAlias(text: string): boolean {
  return FORBIDDEN_COMPLETENESS_RE.test(text);
}

/** Formats `[Public · Fidelity Corroborated (k groups)]` (`k >= 1`). */
export function formatPublicFidelityCorroboratedBadge(corroboratedGroups: number): string {
  const k = Math.max(1, Math.trunc(corroboratedGroups));
  return `[Public · Fidelity Corroborated (${k} groups)]`;
}

export interface ProvenanceBadgeOptions {
  disclosureClass?: string;
  personalMeshSynced?: boolean;
  corroboratedGroups?: number;
}

/** Resolves canonical KU/draft provenance badge per `P10 §2.2`. */
export function formatProvenanceBadge(opts: ProvenanceBadgeOptions): string {
  if (
    opts.personalMeshSynced ||
    opts.disclosureClass === 'NEGOTIATED_ENCRYPTED' ||
    opts.disclosureClass === 'PERSONAL_MESH'
  ) {
    return PROVENANCE_PERSONAL_MESH_ENCRYPTED;
  }
  if (opts.disclosureClass === 'PUBLIC') {
    if (opts.corroboratedGroups && opts.corroboratedGroups > 0) {
      return formatPublicFidelityCorroboratedBadge(opts.corroboratedGroups);
    }
    return PROVENANCE_PUBLIC_UNASSESSED;
  }
  return PROVENANCE_PRIVATE_LOCAL_ONLY;
}

/** Formats `[Sampled Reachable Peers: n]` (`P10 §2.2`). */
export function formatSampledReachablePeersBadge(reachablePeers: number): string {
  const n = Math.max(0, Math.trunc(reachablePeers));
  return `[Sampled Reachable Peers: ${n}]`;
}

export interface CoverageBadgeOptions {
  coverage?: string;
  reachablePeers?: number;
}

/** Resolves canonical query/store coverage badge (`[Local Store Only]` vs `[Sampled Reachable Peers: n]`). */
export function formatCoverageBadge(opts: CoverageBadgeOptions): string {
  const peers = opts.reachablePeers ?? 0;
  if (opts.coverage === 'partial' && peers > 0) {
    return formatSampledReachablePeersBadge(peers);
  }
  if (peers > 0 && opts.coverage !== 'local_only') {
    return formatSampledReachablePeersBadge(peers);
  }
  return COVERAGE_LOCAL_STORE_ONLY;
}

/** Formats AI model qualification status (`model_qualified: false` -> `[Unassessed]`). */
export function formatModelQualificationBadge(modelQualified?: boolean): string {
  return modelQualified ? '[Domain Qualified]' : MODEL_UNASSESSED_BADGE;
}

/** Formats `KQL-CMP-001` deterministic NeedIR compilation status (`P3 §2.3`). */
export function formatNeedCompilerBadge(opts: {
  compiled?: boolean;
  limitations?: readonly string[];
  compilerProfile?: string;
}): string {
  if (opts.limitations?.includes('semantic_mapping_is_hash_derived_not_compiled')) {
    return '[KQL-CMP-001 · Hash-Derived Pre-Compiler Fallback]';
  }
  if (opts.compiled || opts.compilerProfile === 'kql-need-compiler/1') {
    return '[KQL-CMP-001 · Compiled NeedIR (kql-need-compiler/1)]';
  }
  return '[KQL-CMP-001 · Ready for Deterministic FIND → NeedIR]';
}

export type AuxiliarySurfaceKind = 'extension' | 'bot' | 'glasses';

export type CanonicalSurfaceId =
  | 'onebrain-desktop'
  | 'onebrain-web'
  | 'onebrain-cli'
  | 'onebrain-mobile'
  | 'onebrain-extension'
  | 'onebrain-bot'
  | 'onebrain-glasses';

export interface SurfaceCapabilityCeiling {
  surfaceId: CanonicalSurfaceId;
  shortLabel: string;
  auxiliaryKind?: AuxiliarySurfaceKind;
  transport: string;
  privateVaultRead: string;
  obm1AdminArchive: string;
  step2PublicConfirm: string;
  somaticP9Access: string;
  securityNotes: string;
}

/**
 * Normative `P10 §4` per-surface capability ceiling matrix (`obc_surface1`).
 * Phrased without forbidden completeness tokens (`FULL`, `GLOBAL`, `CLOSED`).
 */
export const SURFACE_CAPABILITY_CEILINGS: readonly SurfaceCapabilityCeiling[] = [
  {
    surfaceId: 'onebrain-desktop',
    shortLabel: 'Desktop Host',
    transport: 'In-process / loopback REST+WS',
    privateVaultRead: 'Owner session (scoped local)',
    obm1AdminArchive: 'Scoped via base_archive_file',
    step2PublicConfirm: 'Allowed (typed receipt wizard)',
    somaticP9Access: 'Host foreground only',
    securityNotes: 'Primary local authority surface; tray quit executes graceful shutdown.',
  },
  {
    surfaceId: 'onebrain-web',
    shortLabel: 'Web Surface',
    transport: 'Loopback REST+WS (DR-P3.1/3.2)',
    privateVaultRead: 'Scoped session',
    obm1AdminArchive: 'Disabled by default',
    step2PublicConfirm: 'Allowed (typed receipt wizard)',
    somaticP9Access: 'Denied',
    securityNotes: 'Authenticated loopback session; USABLE_OFFLINE with local Vault.',
  },
  {
    surfaceId: 'onebrain-cli',
    shortLabel: 'CLI Surface',
    transport: 'Loopback REST / Base facade (DR-P3.3)',
    privateVaultRead: 'Scoped CLI capability',
    obm1AdminArchive: 'Allowed via scoped Base facade',
    step2PublicConfirm: 'Allowed (interactive exact match, no --yes)',
    somaticP9Access: 'Denied',
    securityNotes: 'Deterministic scripting & interactive confirmation boundary.',
  },
  {
    surfaceId: 'onebrain-mobile',
    shortLabel: 'Mobile Surface',
    transport: 'In-process base_v1 C-ABI/FFI',
    privateVaultRead: 'Local mobile Vault',
    obm1AdminArchive: 'Mobile archive contract',
    step2PublicConfirm: 'Allowed (mobile two-step consent)',
    somaticP9Access: 'Paired companion host',
    securityNotes: 'Autonomous on-device runtime governed by mobile_build_contract_v1.',
  },
  {
    surfaceId: 'onebrain-extension',
    shortLabel: 'Extension',
    auxiliaryKind: 'extension',
    transport: 'Native-messaging / origin-pinned loopback',
    privateVaultRead: 'Own captured drafts + scoped preview only',
    obm1AdminArchive: 'Prohibited',
    step2PublicConfirm: 'Prohibited (NEG-P10-002)',
    somaticP9Access: 'Prohibited',
    securityNotes:
      'AI-003 sanitized capture strips Cookie, Authorization, URL auth tokens, and password inputs before loopback transmit.',
  },
  {
    surfaceId: 'onebrain-bot',
    shortLabel: 'Bot',
    auxiliaryKind: 'bot',
    transport: 'Isolated DMZ REST (BotPrincipal)',
    privateVaultRead: 'Zero (PUBLIC store only, NEG-P10-003)',
    obm1AdminArchive: 'Prohibited',
    step2PublicConfirm: 'Prohibited (NEG-P10-003)',
    somaticP9Access: 'Prohibited',
    securityNotes:
      'PUBLIC ValidatedStore read only; external channel submissions enter QuarantineRecord with untrusted provenance.',
  },
  {
    surfaceId: 'onebrain-glasses',
    shortLabel: 'Glasses',
    auxiliaryKind: 'glasses',
    transport: 'Paired mTLS/Noise (obp/companion-hud/1)',
    privateVaultRead: 'Active HUD context cards only',
    obm1AdminArchive: 'Prohibited',
    step2PublicConfirm: 'Two-step physical + readback only',
    somaticP9Access: 'Paired SomaticImmediate/1',
    securityNotes:
      'Zero at-rest master secrets; <= 40 ms wear-sensor doff-lock blanks HUD & flushes Tier M0; voice-only confirm prohibited.',
  },
];
