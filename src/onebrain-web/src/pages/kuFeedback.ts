import { KuError } from "../api/ku";

export function isStaleSharePreparationError(error: unknown): boolean {
  const reasons =
    error instanceof KuError ? (error.failure?.limitations ?? []) : [];
  return (
    reasons.includes("prepared_share_missing") ||
    reasons.includes("public_ku_consent_expired") ||
    reasons.includes("saved_access_changed") ||
    reasons.includes("outbox_unavailable_reprepare_same_intent")
  );
}

export function describeKuError(error: unknown) {
  const failure = error instanceof KuError ? error.failure : undefined;
  const reasons = failure?.limitations ?? [];
  const deadline = failure?.code === "ResourceExhausted" && reasons.includes("deadline");
  const diagnostics = reasons.filter(reason => reason.startsWith("schema: ") || reason.startsWith("grounding: "));
  const conceptLabel = reasons.includes("concept_label");
  const duplicateId = reasons.includes("duplicate_id");
  const shareNetworkUnavailable =
    failure?.code === "CapabilityDisabled" &&
    reasons.includes("ku_public_share_network_unavailable");
  const staleSharePreparation =
    reasons.includes("prepared_share_missing") ||
    reasons.includes("public_ku_consent_expired");
  const savedAccessChanged = reasons.includes("saved_access_changed");
  const savedKuNotAccessible = reasons.includes("saved_ku_not_accessible");
  const unsupportedPublicKu = reasons.includes("unsupported_public_ku");
  const preparedShareLimit = reasons.includes("prepared_limit");
  const shareConfirmOutboxUnavailable = reasons.includes(
    "outbox_unavailable_reprepare_same_intent",
  );
  const shareIntentOutboxUnavailable = reasons.includes("outbox_unavailable");
  const publicStoreUnavailable = reasons.includes("public_store_unavailable");
  const provenanceUnavailable = reasons.includes("provenance_unavailable");
  const publicObjectNotFound = reasons.includes("public_object_not_found");
  const shareIntentNotFound = reasons.includes("intent_not_found");
  const invalidCandidate = diagnostics.length > 0 || reasons.includes("oneof") || conceptLabel || duplicateId;
  const technical = [
    error instanceof Error ? error.message : "Local operation failed",
    failure?.code,
    ...reasons,
    failure
      ? `Retryable: ${failure.retryable}; reconcile before retry: ${failure.reconcile_before_retry}`
      : "",
  ].filter(Boolean).join(" · ");
  return {
    title: deadline
      ? "Encoding timed out"
      : shareNetworkUnavailable
        ? "Public KU share network runtime is disabled"
        : staleSharePreparation
          ? "Prepared Public KU share preview is no longer valid"
          : savedAccessChanged
            ? "Saved private KU access changed before share confirmation"
            : savedKuNotAccessible
              ? "Saved private KU is no longer accessible for share preparation"
              : unsupportedPublicKu
                ? "KU is not eligible for D-045 Public share"
                : preparedShareLimit
                  ? "In-memory Public KU share preparation limit reached"
                  : shareConfirmOutboxUnavailable
                    ? "Durable outbound share outbox was unavailable during confirmation"
                    : shareIntentOutboxUnavailable
                      ? "Durable outbound share outbox is currently unavailable"
                      : publicStoreUnavailable
                        ? "Local canonical public store is currently unavailable"
                        : provenanceUnavailable
                          ? "Accepted Public KU transport provenance is currently unavailable"
                          : publicObjectNotFound
                            ? "Accepted Public KU was not found in the local public store"
                            : shareIntentNotFound
                              ? "Outbound Public KU share intent was not found"
                              : duplicateId
                                ? "AI output repeats an internal identifier"
                                : conceptLabel
                                  ? "AI concept label differs from its source evidence"
                                  : invalidCandidate
                                    ? "AI output does not match the KU schema"
                                    : "Local operation could not finish",
    explanation: deadline
      ? "The host exhausted the encoding time budget before returning a validated preview. The current experimental Ollama workflow has a 600-second (10-minute) budget, including worker startup, model inference, any repair call and validation."
      : shareNetworkUnavailable
        ? "The host keeps networking disabled by default. Confirming a Public KU share or inspecting outbound/accepted peer share state requires an active opt-in VNextNetworkRuntime attached to this host."
        : staleSharePreparation
          ? "The host rejected confirmation because this single-use in-memory share preparation expired after its 15-minute (900-second) window, was evicted under the 16-item in-memory preparation limit, was already used once, or was cleared when the host restarted."
          : savedAccessChanged
            ? "The host rechecked the saved private KU during confirmation and rejected disclosure because the artifact is no longer accessible to the current principal or dataset, or no longer matches the exact LOCAL_ONLY accepted_known ObjectCID and canonical bytes from preparation."
            : savedKuNotAccessible
              ? "The host rejected share preparation because the requested private ObjectCID could not be read from the current local Vault dataset under this principal and session."
              : unsupportedPublicKu
                ? "D-045 supports only a single manual statement with one resolved Registry predicate and one text literal, with no object references, source spans, constraints, qualifiers, extensions or resource limits, and requires a non-zero recipient NodeID, selector, namespace and loopback peer address. The host fails closed rather than stripping private or meaning-bearing content."
                : preparedShareLimit
                  ? "The host already holds the maximum of 16 unexpired single-use Public KU share previews in memory. It rejected creating another preview until an existing preparation is confirmed, expires after its 15-minute (900-second) window, or is cleared by a host restart."
                  : shareConfirmOutboxUnavailable
                    ? "The host verified and consumed the single-use share preparation, but its active VNextNetworkRuntime could not confirm durable outbox persistence for the outbound transfer intent."
                    : shareIntentOutboxUnavailable
                      ? "The host's active VNextNetworkRuntime could not read the local durable outbound intent store while checking this Intent ID."
                      : publicStoreUnavailable
                        ? "The host's active VNextNetworkRuntime could not read the canonical verified public store (vnext_verified.redb) while inspecting the requested Public ObjectCID."
                        : provenanceUnavailable
                          ? "The host decoded the accepted Public KU from vnext_verified.redb, but its active VNextNetworkRuntime could not read the authenticated transport/selector provenance index for the requested selector CID."
                          : publicObjectNotFound
                            ? "This host's canonical verified public store (vnext_verified.redb) does not contain an accepted Public KU with the requested Public ObjectCID. Peer reconciliation may still be pending, or the CID may refer to a private LOCAL_ONLY artifact or a different host."
                            : shareIntentNotFound
                              ? "This host's durable outbox does not contain an outbound transfer intent with the requested Intent ID. Preparing a preview computes the deterministic Intent ID in memory (enqueued: false), but the durable intent is created only after Confirm public share succeeds on this sender host."
                              : duplicateId
                                ? "The model reused an identifier within a collection of concepts, statements or coverage entries. Distinct items need distinct keys; references must point to the intended item. Each required unit needs one coverage entry. No validated preview was produced."
                                : conceptLabel
                                  ? "A concept.label returned by the model does not exactly match its evidence.quote. The label must copy the source wording, including case, accents and spelling. No validated preview was produced. This error does not establish that the concept is missing from the Registry or that your sentence is factually wrong."
                                  : invalidCandidate
                                    ? "The host rejected the model's structured output. No validated preview was produced. The schema issues below identify fields to repair; they do not judge whether your source is factually true."
                                    : "The host did not return a successful result for this request. Read the technical reason below before deciding whether to retry.",
    context: deadline
      ? "Here, rate_limited is the API category for ResourceExhausted; deadline identifies a time limit. It does not indicate an AI account quota, prove that RAM ran out, or mean your sentence is incorrect. The reply does not identify the exact internal step that timed out."
      : shareNetworkUnavailable
        ? "No payload was enqueued or sent. Any unexpired prepared share preview remains available in memory."
        : staleSharePreparation
          ? "No payload was enqueued or sent by this failed confirmation, and the saved private artifact remains unchanged. The stale preview has been cleared from this page."
          : savedAccessChanged
            ? "No payload was enqueued or sent, and the invalidated share preview has been cleared from this page."
            : savedKuNotAccessible
              ? "No public preview, public object or outbound transfer intent was created, and existing saved knowledge remains unchanged."
              : unsupportedPublicKu
                ? "No public preview, public object or outbound transfer intent was created, and the saved private artifact remains unchanged."
                : preparedShareLimit
                  ? "Here, rate_limited (ResourceExhausted) reflects the 16-item in-memory share preparation cap, not an AI model timeout or quota. No new preview or outbound intent was created, and existing unexpired preparations remain in memory."
                  : shareConfirmOutboxUnavailable
                    ? "Here, dependency_unavailable (DependencyUnavailable) reflects local outbox storage availability, not an AI model or Base operation failure. The single-use preview has been cleared from this page; re-preparing the same saved artifact with the same recipient, selector and namespace reproduces the exact deterministic Intent ID and Public ObjectCID."
                    : shareIntentOutboxUnavailable
                      ? "Here, dependency_unavailable (DependencyUnavailable) reflects local outbox storage availability, not an AI model failure or a Base operation that uses Reconcile operation. No new outbound intent or network transfer was started by this status check."
                      : publicStoreUnavailable
                        ? "Here, dependency_unavailable (DependencyUnavailable) reflects local canonical public store availability, not an AI model failure or a Base operation that uses Reconcile operation. Inspecting a Public KU is a local read and does not modify private Vault artifacts."
                        : provenanceUnavailable
                          ? "Here, dependency_unavailable (DependencyUnavailable) reflects local transport provenance store availability, not an AI model failure, original-source verification or a Base operation that uses Reconcile operation. Inspecting a Public KU does not expose private Vault artifacts."
                          : publicObjectNotFound
                            ? "Inspecting a Public KU reads only already-accepted public records on this host; it does not fetch from a remote peer or expose private Vault artifacts."
                            : shareIntentNotFound
                              ? "No new outbound intent or network transfer was started by this status check."
                              : invalidCandidate
                                ? "DependencyUnavailable is the transport category for this failure; it does not mean Ollama is disconnected. The automatic repair allowance is bounded; checking the recorded outcome will not run another repair."
                                : "An interrupted response does not establish whether a write completed. Existing saved knowledge remains separate from this request.",
    recovery: preparedShareLimit
      ? "Confirm an existing prepared share preview if intended, wait for older 15-minute preparations to expire, or restart the host to clear in-memory preparations before clicking Prepare public share preview again."
      : shareConfirmOutboxUnavailable
        ? "Click Inspect share intent first to check whether the deterministic Intent ID was recorded, or click Prepare public share preview again with the same recipient, selector and namespace to create a fresh preview for the same deterministic Intent ID before confirming."
        : shareIntentOutboxUnavailable
          ? "Verify that the host's network outbox storage is healthy and accessible (or restart the host if its local outbox handle is degraded), then click Inspect share intent again."
          : publicStoreUnavailable
            ? "Verify that the host's canonical verified public store (vnext_verified.redb) is healthy and accessible (or restart the host if its storage handle is degraded), then click Inspect accepted Public KU again."
            : provenanceUnavailable
              ? "Verify the selector CID and that the host's network provenance store is healthy and accessible (or restart the host if its storage handle is degraded), then click Inspect accepted Public KU again."
              : failure?.reconcile_before_retry || (error instanceof KuError && error.uncertain)
                ? "Use Reconcile operation to read the host's recorded outcome. It does not rerun AI. After a failed or canceled outcome, start a new attempt. If work remains pending, cancel it before starting again."
                : staleSharePreparation
                  ? "Click Prepare public share preview again to create and review a fresh preview before confirming public disclosure."
                  : savedAccessChanged
                    ? "Use Search / list and Inspect to verify the saved LOCAL_ONLY artifact under the current host session, then click Prepare public share preview again before confirming."
                    : savedKuNotAccessible
                      ? "Use Search / list and Inspect to load an accepted LOCAL_ONLY artifact under the current host session before clicking Prepare public share preview."
                      : unsupportedPublicKu
                        ? "Inspect a supported accepted LOCAL_ONLY manual KU (one resolved predicate and one text literal) and verify the recipient NodeID, loopback address (127.0.0.1:<port>), selector and namespace before clicking Prepare public share preview."
                        : publicObjectNotFound
                          ? "Verify that you entered the new Public ObjectCID (not the private ObjectCID), check Outbound share intent status on the sender until it reaches Acknowledged, and confirm the matching selector CID before clicking Inspect accepted Public KU again."
                          : shareIntentNotFound
                            ? "If you have only prepared a preview, complete Confirm public share first so the host enqueues the durable intent; otherwise verify that the 64-character hex Intent ID belongs to this sender host."
                            : "Check the technical reason and refresh host status. Correct the reported problem before starting another request.",
    technical,
    diagnostics,
  };
}
