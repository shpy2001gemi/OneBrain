/**
 * Pillar 10 §4 / §4.1 — Scoped `obc_surface1` Extension Capability Client.
 *
 * Enforces:
 * - `obc_surface1.<claims>.<mac>` token validation (`surface_kind === "extension"`)
 * - Fail-closed rejection of `obm1.` admin/archive tokens
 * - Hard blocks on bulk `PrivateVault` & Base ABI routes (`/api/kus`, `/api/export`,
 *   `/api/backup`, `/api/restore`, `/api/base/v1/*`)
 * - Hard blocks on `P9` somatic/BCI endpoints (`T-BCI-04`)
 * - Hard block on step-2 `PUBLIC` confirmation (`NEG-P10-002`), returning a
 *   deterministic `HostConfirmationHandoff` to `http://127.0.0.1:4280/ku?review_draft=...`
 * - Canonical `RUN-004` / `P0-ADR-001` epistemic badges (never `FULL`, `GLOBAL`, or `CLOSED`)
 */

import {
  sanitizeWebCapture,
  type RawWebCaptureInput,
  type SanitizedWebCaptureV1,
} from "./sanitizer.ts";

export const LOOPBACK_HOST_ORIGIN = "http://127.0.0.1:4280";

export const CANONICAL_EPISTEMIC_BADGES = [
  "[Private · Local Only]",
  "[Personal Mesh · Encrypted]",
  "[Public · Unassessed]",
  "[Local Store Only]",
] as const;

export type CanonicalEpistemicBadge =
  | (typeof CANONICAL_EPISTEMIC_BADGES)[number]
  | `[Public · Fidelity Corroborated (${number} groups)]`
  | `[Sampled Reachable Peers: ${number}]`;

const FORBIDDEN_LEGACY_BADGE_TOKENS = ["FULL", "GLOBAL", "CLOSED"] as const;

const ALLOWED_EXTENSION_PERMISSIONS: ReadonlySet<string> = new Set([
  "extension:capture:write",
  "extension:drafts:read",
]);

const PROHIBITED_BULK_VAULT_PREFIXES: readonly string[] = [
  "/api/kus",
  "/api/export",
  "/api/backup",
  "/api/restore",
  "/api/base/v1",
  "/api/archive",
  "/api/vnext/archive",
  "/api/obm",
];

const PROHIBITED_P9_SOMATIC_PREFIXES: readonly string[] = [
  "/api/p9",
  "/api/vnext/p9",
  "/api/bci",
  "/api/vnext/bci",
  "/api/somatic",
  "/api/vnext/somatic",
];

export interface ObcSurfaceClaimsV1 {
  readonly surface_kind: string;
  readonly allowed_origin?: string;
  readonly permissions?: readonly string[];
  readonly exp_ms?: number;
}

export interface ValidatedExtensionToken {
  readonly raw: string;
  readonly claims: {
    readonly surface_kind: "extension";
    readonly allowed_origin: string;
    readonly permissions: readonly string[];
    readonly exp_ms?: number;
  };
  readonly mac_bytes_len: number;
}

export interface HostConfirmationHandoff {
  readonly blocked: true;
  readonly invariant_id: "NEG-P10-002";
  readonly draft_id: string;
  readonly handoff_url: string;
  readonly required_host_surfaces: readonly ["onebrain-desktop", "onebrain-web"];
  readonly epistemic_badge: "[Public · Unassessed]";
}

export interface CapturedDraftReceiptV1 {
  readonly draft_id: string;
  readonly artifact_cid?: string;
  readonly epistemic_badge: CanonicalEpistemicBadge;
  readonly reachability_badge: CanonicalEpistemicBadge;
  readonly model_qualified: boolean;
}

export interface ScopedDraftPreviewV1 {
  readonly draft_id: string;
  readonly title: string;
  readonly sanitized_url: string;
  readonly summary: string;
  readonly epistemic_badge: CanonicalEpistemicBadge;
  readonly reachability_badge: CanonicalEpistemicBadge;
  readonly model_qualified: boolean;
}

export type FetchLike = (
  input: string,
  init?: {
    method?: string;
    headers?: Record<string, string>;
    body?: string;
  },
) => Promise<{
  readonly ok: boolean;
  readonly status: number;
  json(): Promise<unknown>;
}>;

export class ExtensionCapabilityError extends Error {
  readonly code: string;
  readonly handoff?: HostConfirmationHandoff;

  constructor(code: string, message: string, handoff?: HostConfirmationHandoff) {
    super(message);
    this.name = "ExtensionCapabilityError";
    this.code = code;
    this.handoff = handoff;
  }
}

function decodeBase64UrlBytes(segment: string): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/.test(segment)) {
    throw new ExtensionCapabilityError(
      "INVALID_SURFACE_TOKEN_ENCODING",
      "Token segment contains non-base64url characters",
    );
  }
  const padded = segment
    .replace(/-/g, "+")
    .replace(/_/g, "/")
    .padEnd(Math.ceil(segment.length / 4) * 4, "=");

  if (typeof globalThis.atob === "function") {
    const binary = globalThis.atob(padded);
    const out = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      out[i] = binary.charCodeAt(i);
    }
    return out;
  }

  throw new ExtensionCapabilityError(
    "BASE64_DECODER_UNAVAILABLE",
    "Runtime lacks standard base64 decoding support",
  );
}

function decodeMacSegmentLength(macSegment: string): number {
  if (/^[0-9a-fA-F]{64}$/.test(macSegment)) {
    return 32;
  }
  const bytes = decodeBase64UrlBytes(macSegment);
  return bytes.byteLength;
}

export function validateExtensionSurfaceToken(
  token: string,
  nowMs: number = Date.now(),
): ValidatedExtensionToken {
  const trimmed = token.trim();
  if (!trimmed) {
    throw new ExtensionCapabilityError(
      "MISSING_SURFACE_TOKEN",
      "Extension requires a scoped obc_surface1 capability token",
    );
  }

  if (trimmed.toLowerCase().startsWith("obm1.") || trimmed.toLowerCase() === "obm1") {
    throw new ExtensionCapabilityError(
      "OBM1_ADMIN_TOKEN_PROHIBITED",
      "NEG-P10-002: obm1 management/archive tokens are strictly prohibited in onebrain-extension",
    );
  }

  const parts = trimmed.split(".");
  if (parts.length !== 3 || parts[0] !== "obc_surface1") {
    throw new ExtensionCapabilityError(
      "INVALID_SURFACE_TOKEN_SHAPE",
      "Expected token shape obc_surface1.<claims>.<mac>",
    );
  }

  const claimsBytes = decodeBase64UrlBytes(parts[1]!);
  let parsedClaims: unknown;
  try {
    const jsonText = new TextDecoder("utf-8", { fatal: true }).decode(claimsBytes);
    parsedClaims = JSON.parse(jsonText);
  } catch {
    throw new ExtensionCapabilityError(
      "INVALID_SURFACE_TOKEN_CLAIMS",
      "obc_surface1 claims segment is not valid UTF-8 JSON",
    );
  }

  if (typeof parsedClaims !== "object" || parsedClaims === null) {
    throw new ExtensionCapabilityError(
      "INVALID_SURFACE_TOKEN_CLAIMS",
      "obc_surface1 claims must decode to a JSON object",
    );
  }

  const claims = parsedClaims as ObcSurfaceClaimsV1;
  if (claims.surface_kind !== "extension") {
    throw new ExtensionCapabilityError(
      "WRONG_SURFACE_KIND",
      `Expected surface_kind === "extension" (got ${String(claims.surface_kind)})`,
    );
  }

  const allowedOrigin = claims.allowed_origin ?? LOOPBACK_HOST_ORIGIN;
  if (allowedOrigin !== LOOPBACK_HOST_ORIGIN) {
    throw new ExtensionCapabilityError(
      "NON_LOOPBACK_TOKEN_ORIGIN",
      `Extension token origin must be pinned to ${LOOPBACK_HOST_ORIGIN}`,
    );
  }

  const permissions = claims.permissions ?? [
    "extension:capture:write",
    "extension:drafts:read",
  ];
  if (!Array.isArray(permissions) || permissions.length === 0) {
    throw new ExtensionCapabilityError(
      "EMPTY_SURFACE_PERMISSIONS",
      "obc_surface1 token must declare at least one scoped extension permission",
    );
  }
  for (const perm of permissions) {
    if (typeof perm !== "string" || !ALLOWED_EXTENSION_PERMISSIONS.has(perm)) {
      throw new ExtensionCapabilityError(
        "SURFACE_CEILING_EXCEEDED",
        `Permission "${String(perm)}" exceeds onebrain-extension capability ceiling`,
      );
    }
  }

  if (claims.exp_ms !== undefined) {
    if (typeof claims.exp_ms !== "number" || !Number.isFinite(claims.exp_ms)) {
      throw new ExtensionCapabilityError(
        "INVALID_TOKEN_EXPIRY",
        "exp_ms claim must be a finite epoch millisecond timestamp",
      );
    }
    if (nowMs >= claims.exp_ms) {
      throw new ExtensionCapabilityError(
        "SURFACE_TOKEN_EXPIRED",
        "obc_surface1 capability token has expired",
      );
    }
  }

  const macBytesLen = decodeMacSegmentLength(parts[2]!);
  if (macBytesLen !== 32) {
    throw new ExtensionCapabilityError(
      "INVALID_SURFACE_TOKEN_MAC",
      `obc_surface1 BLAKE3 MAC must be 32 bytes (got ${macBytesLen} bytes)`,
    );
  }

  return {
    raw: trimmed,
    claims: {
      surface_kind: "extension",
      allowed_origin: allowedOrigin,
      permissions: [...permissions],
      exp_ms: claims.exp_ms,
    },
    mac_bytes_len: macBytesLen,
  };
}

export function assertCanonicalEpistemicBadge(badge: string): CanonicalEpistemicBadge {
  for (const forbidden of FORBIDDEN_LEGACY_BADGE_TOKENS) {
    if (badge.toUpperCase().includes(forbidden)) {
      throw new ExtensionCapabilityError(
        "LEGACY_BADGE_PROHIBITED",
        `RUN-004 / P10 §2.2 forbids legacy ${forbidden} alias in surface badges: "${badge}"`,
      );
    }
  }

  if (
    badge === "[Private · Local Only]" ||
    badge === "[Personal Mesh · Encrypted]" ||
    badge === "[Public · Unassessed]" ||
    badge === "[Local Store Only]" ||
    /^\[Public · Fidelity Corroborated \(\d+ groups\)\]$/.test(badge) ||
    /^\[Sampled Reachable Peers: \d+\]$/.test(badge)
  ) {
    return badge as CanonicalEpistemicBadge;
  }

  throw new ExtensionCapabilityError(
    "UNKNOWN_EPISTEMIC_BADGE",
    `Unrecognized epistemic/reachability badge: "${badge}"`,
  );
}

export function buildHostConfirmationHandoff(draftId: string): HostConfirmationHandoff {
  const normalizedId = draftId.trim();
  if (!normalizedId || !/^[A-Za-z0-9._:-]+$/.test(normalizedId)) {
    throw new ExtensionCapabilityError(
      "INVALID_DRAFT_ID",
      "Draft ID must be a non-empty safe identifier",
    );
  }
  return {
    blocked: true,
    invariant_id: "NEG-P10-002",
    draft_id: normalizedId,
    handoff_url: `${LOOPBACK_HOST_ORIGIN}/ku?review_draft=${encodeURIComponent(normalizedId)}`,
    required_host_surfaces: ["onebrain-desktop", "onebrain-web"],
    epistemic_badge: "[Public · Unassessed]",
  };
}

function matchesPrefixBoundary(pathname: string, prefix: string): boolean {
  return pathname === prefix || pathname.startsWith(`${prefix}/`);
}

function extractDraftIdFromConfirmPath(pathname: string): string | null {
  const match = /^\/api\/vnext\/extension\/drafts\/([^/]+)\/(?:confirm|publish|public-confirm|confirm-public)$/i.exec(
    pathname,
  );
  return match?.[1] ? decodeURIComponent(match[1]) : null;
}

export interface ExtensionCapabilityClientOptions {
  readonly token: string;
  readonly baseOrigin?: string;
  readonly fetchImpl?: FetchLike;
  readonly nowFn?: () => number;
}

export class ExtensionCapabilityClient {
  readonly baseOrigin: string;
  readonly validatedToken: ValidatedExtensionToken;
  readonly #fetchImpl: FetchLike;
  readonly #nowFn: () => number;

  constructor(options: ExtensionCapabilityClientOptions) {
    const baseOrigin = options.baseOrigin ?? LOOPBACK_HOST_ORIGIN;
    if (baseOrigin !== LOOPBACK_HOST_ORIGIN) {
      throw new ExtensionCapabilityError(
        "NON_LOOPBACK_ENDPOINT_PROHIBITED",
        `onebrain-extension must pin API traffic strictly to ${LOOPBACK_HOST_ORIGIN}`,
      );
    }
    this.#nowFn = options.nowFn ?? (() => Date.now());
    this.baseOrigin = baseOrigin;
    this.validatedToken = validateExtensionSurfaceToken(options.token, this.#nowFn());

    const defaultFetch = globalThis.fetch?.bind(globalThis) as FetchLike | undefined;
    const chosenFetch = options.fetchImpl ?? defaultFetch;
    if (!chosenFetch) {
      throw new ExtensionCapabilityError(
        "FETCH_UNAVAILABLE",
        "No fetch implementation available",
      );
    }
    this.#fetchImpl = chosenFetch;
  }

  /**
   * Hard-blocks in-extension step-2 `PUBLIC` confirmation (`NEG-P10-002`) without
   * issuing any network request, returning the deterministic host handoff descriptor.
   */
  confirmPublicPublication(draftId: string): HostConfirmationHandoff {
    return buildHostConfirmationHandoff(draftId);
  }

  assertEndpointPermitted(method: string, pathOrUrl: string): URL {
    const target = pathOrUrl.startsWith("http://") || pathOrUrl.startsWith("https://")
      ? new URL(pathOrUrl)
      : new URL(pathOrUrl, this.baseOrigin);

    if (target.origin !== LOOPBACK_HOST_ORIGIN) {
      throw new ExtensionCapabilityError(
        "NON_LOOPBACK_ENDPOINT_PROHIBITED",
        `Blocked non-loopback request to ${target.origin}`,
      );
    }

    const pathname = target.pathname;

    for (const somaticPrefix of PROHIBITED_P9_SOMATIC_PREFIXES) {
      if (matchesPrefixBoundary(pathname.toLowerCase(), somaticPrefix)) {
        throw new ExtensionCapabilityError(
          "P9_SOMATIC_ACCESS_PROHIBITED",
          `T-BCI-04: P9 somatic endpoint "${pathname}" is hard-blocked in onebrain-extension`,
        );
      }
    }

    for (const vaultPrefix of PROHIBITED_BULK_VAULT_PREFIXES) {
      if (matchesPrefixBoundary(pathname.toLowerCase(), vaultPrefix)) {
        throw new ExtensionCapabilityError(
          "BULK_VAULT_ENDPOINT_PROHIBITED",
          `NEG-P10-002: Bulk PrivateVault / Base endpoint "${pathname}" is prohibited in onebrain-extension`,
        );
      }
    }

    const confirmDraftId = extractDraftIdFromConfirmPath(pathname);
    if (
      confirmDraftId !== null ||
      /(?:\/|^)(?:publish|confirm-public|public-confirm|step2-confirm)(?:\/|$)/i.test(
        pathname,
      )
    ) {
      const handoff = confirmDraftId
        ? buildHostConfirmationHandoff(confirmDraftId)
        : buildHostConfirmationHandoff("pending");
      throw new ExtensionCapabilityError(
        "PUBLIC_CONFIRM_PROHIBITED",
        `NEG-P10-002: Step-2 PUBLIC confirmation is prohibited in onebrain-extension; open ${handoff.handoff_url}`,
        handoff,
      );
    }

    const normalizedMethod = method.trim().toUpperCase();
    const isCaptureWrite =
      normalizedMethod === "POST" && pathname === "/api/vnext/extension/capture";
    const isDraftRead =
      normalizedMethod === "GET" &&
      /^\/api\/vnext\/extension\/drafts\/[A-Za-z0-9._:-]+$/.test(pathname);

    if (!isCaptureWrite && !isDraftRead) {
      throw new ExtensionCapabilityError(
        "ENDPOINT_OUTSIDE_SURFACE_CEILING",
        `Endpoint ${normalizedMethod} ${pathname} is outside onebrain-extension obc_surface1 ceiling`,
      );
    }

    validateExtensionSurfaceToken(this.validatedToken.raw, this.#nowFn());
    return target;
  }

  async rawScopedRequest(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<unknown> {
    const target = this.assertEndpointPermitted(method, path);
    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.validatedToken.raw}`,
      Accept: "application/json",
    };
    let encodedBody: string | undefined;
    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
      encodedBody = JSON.stringify(body);
    }

    const response = await this.#fetchImpl(target.toString(), {
      method: method.toUpperCase(),
      headers,
      body: encodedBody,
    });

    if (!response.ok) {
      throw new ExtensionCapabilityError(
        "API_REQUEST_FAILED",
        `onebrain-api returned HTTP ${response.status} for ${method.toUpperCase()} ${target.pathname}`,
      );
    }

    return response.json();
  }

  async submitCapture(
    capture: SanitizedWebCaptureV1 | RawWebCaptureInput,
  ): Promise<CapturedDraftReceiptV1> {
    if (!this.validatedToken.claims.permissions.includes("extension:capture:write")) {
      throw new ExtensionCapabilityError(
        "MISSING_CAPTURE_WRITE_PERMISSION",
        "Token lacks extension:capture:write permission",
      );
    }

    const sanitized: SanitizedWebCaptureV1 =
      "schema_version" in capture && capture.schema_version === "onebrain.web_capture.v1"
        ? capture
        : sanitizeWebCapture(capture);

    assertCanonicalEpistemicBadge(sanitized.epistemic_badge);

    const payload = (await this.rawScopedRequest(
      "POST",
      "/api/vnext/extension/capture",
      sanitized,
    )) as Record<string, unknown>;

    const draftId = String(payload.draft_id ?? "");
    if (!draftId) {
      throw new ExtensionCapabilityError(
        "MALFORMED_CAPTURE_RESPONSE",
        "Capture response missing draft_id",
      );
    }

    const epistemicBadge = assertCanonicalEpistemicBadge(
      String(payload.epistemic_badge ?? "[Private · Local Only]"),
    );
    const reachabilityBadge = assertCanonicalEpistemicBadge(
      String(payload.reachability_badge ?? "[Local Store Only]"),
    );

    return {
      draft_id: draftId,
      artifact_cid:
        typeof payload.artifact_cid === "string" ? payload.artifact_cid : undefined,
      epistemic_badge: epistemicBadge,
      reachability_badge: reachabilityBadge,
      model_qualified: Boolean(payload.model_qualified ?? false),
    };
  }

  async getDraft(draftId: string): Promise<ScopedDraftPreviewV1> {
    if (!this.validatedToken.claims.permissions.includes("extension:drafts:read")) {
      throw new ExtensionCapabilityError(
        "MISSING_DRAFT_READ_PERMISSION",
        "Token lacks extension:drafts:read permission",
      );
    }

    const normalizedId = draftId.trim();
    if (!normalizedId || !/^[A-Za-z0-9._:-]+$/.test(normalizedId)) {
      throw new ExtensionCapabilityError(
        "INVALID_DRAFT_ID",
        "Draft ID must be a safe non-empty identifier",
      );
    }

    const payload = (await this.rawScopedRequest(
      "GET",
      `/api/vnext/extension/drafts/${encodeURIComponent(normalizedId)}`,
    )) as Record<string, unknown>;

    const epistemicBadge = assertCanonicalEpistemicBadge(
      String(payload.epistemic_badge ?? "[Private · Local Only]"),
    );
    const reachabilityBadge = assertCanonicalEpistemicBadge(
      String(payload.reachability_badge ?? "[Local Store Only]"),
    );

    return {
      draft_id: String(payload.draft_id ?? normalizedId),
      title: String(payload.title ?? ""),
      sanitized_url: String(payload.sanitized_url ?? ""),
      summary: String(payload.summary ?? ""),
      epistemic_badge: epistemicBadge,
      reachability_badge: reachabilityBadge,
      model_qualified: Boolean(payload.model_qualified ?? false),
    };
  }
}
