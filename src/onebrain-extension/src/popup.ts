/**
 * Pillar 10 §2.2 & §4.1 — Popup View-Model & Host Confirmation Handoff UI.
 *
 * Renders canonical `RUN-004` / `P0-ADR-001` badges (`[Private · Local Only]`,
 * `[Public · Unassessed]`, `[Local Store Only]`) and exposes a host handoff link
 * rather than an in-extension `PUBLIC` publish confirmation button (`NEG-P10-002`).
 */

import {
  assertCanonicalEpistemicBadge,
  buildHostConfirmationHandoff,
  type CanonicalEpistemicBadge,
  type CapturedDraftReceiptV1,
  type HostConfirmationHandoff,
} from "./capabilityClient.ts";
import type { SanitizedWebCaptureV1 } from "./sanitizer.ts";

export interface PopupViewModel {
  readonly draftId: string | null;
  readonly epistemicBadge: CanonicalEpistemicBadge;
  readonly reachabilityBadge: CanonicalEpistemicBadge;
  readonly redactedElementCount: number;
  readonly strippedParamNames: readonly string[];
  readonly hostConfirmationHandoff: HostConfirmationHandoff | null;
}

export function createInitialPopupViewModel(): PopupViewModel {
  return {
    draftId: null,
    epistemicBadge: "[Private · Local Only]",
    reachabilityBadge: "[Local Store Only]",
    redactedElementCount: 0,
    strippedParamNames: [],
    hostConfirmationHandoff: null,
  };
}

export function applyCapturedDraftToPopup(
  capture: SanitizedWebCaptureV1,
  receipt: CapturedDraftReceiptV1,
): PopupViewModel {
  const epistemicBadge = assertCanonicalEpistemicBadge(receipt.epistemic_badge);
  const reachabilityBadge = assertCanonicalEpistemicBadge(receipt.reachability_badge);
  const handoff = buildHostConfirmationHandoff(receipt.draft_id);

  return {
    draftId: receipt.draft_id,
    epistemicBadge,
    reachabilityBadge,
    redactedElementCount: capture.redacted_element_count,
    strippedParamNames: [...capture.stripped_param_names],
    hostConfirmationHandoff: handoff,
  };
}
