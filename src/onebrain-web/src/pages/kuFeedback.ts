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

type ShareErrorFeedback = {
  title: string;
  explanation: string;
  context: string;
  recovery?: string;
};

const STALE_SHARE_PREPARATION_FEEDBACK: ShareErrorFeedback = {
  title: "Prepared Public KU share preview is no longer valid",
  explanation:
    "The host rejected confirmation because this single-use in-memory share preparation expired after its 15-minute (900-second) window, was evicted under the 16-item in-memory preparation limit, was already used once, or was cleared when the host restarted.",
  context:
    "No payload was enqueued or sent by this failed confirmation, and the saved private artifact remains unchanged. The stale preview has been cleared from this page.",
  recovery:
    "Click Prepare public share preview again to create and review a fresh preview before confirming public disclosure.",
};

const SHARE_ERROR_FEEDBACK: Record<string, ShareErrorFeedback> = {
  prepared_share_missing: STALE_SHARE_PREPARATION_FEEDBACK,
  public_ku_consent_expired: STALE_SHARE_PREPARATION_FEEDBACK,
  saved_access_changed: {
    title: "Saved private KU access changed before share confirmation",
    explanation:
      "The host rechecked the saved private KU during confirmation and rejected disclosure because the artifact is no longer accessible to the current principal or dataset, or no longer matches the exact LOCAL_ONLY accepted_known ObjectCID and canonical bytes from preparation.",
    context:
      "No payload was enqueued or sent, and the invalidated share preview has been cleared from this page.",
    recovery:
      "Use Search / list and Inspect to verify the saved LOCAL_ONLY artifact under the current host session, then click Prepare public share preview again before confirming.",
  },
  saved_ku_not_accessible: {
    title: "Saved private KU is no longer accessible for share preparation",
    explanation:
      "The host rejected share preparation because the requested private ObjectCID could not be read from the current local Vault dataset under this principal and session.",
    context:
      "No public preview, public object or outbound transfer intent was created, and existing saved knowledge remains unchanged.",
    recovery:
      "Use Search / list and Inspect to load an accepted LOCAL_ONLY artifact under the current host session before clicking Prepare public share preview.",
  },
  unsupported_public_ku: {
    title: "KU is not eligible for D-045 Public share",
    explanation:
      "D-045 supports only a single manual statement with one resolved Registry predicate and one text literal, with no object references, source spans, constraints, qualifiers, extensions or resource limits, and requires a non-zero recipient NodeID, selector, namespace and loopback peer address. The host fails closed rather than stripping private or meaning-bearing content.",
    context:
      "No public preview, public object or outbound transfer intent was created, and the saved private artifact remains unchanged.",
    recovery:
      "Inspect a supported accepted LOCAL_ONLY manual KU (one resolved predicate and one text literal) and verify the recipient NodeID, loopback address (127.0.0.1:<port>), selector and namespace before clicking Prepare public share preview.",
  },
  prepared_limit: {
    title: "In-memory Public KU share preparation limit reached",
    explanation:
      "The host already holds the maximum of 16 unexpired single-use Public KU share previews in memory. It rejected creating another preview until an existing preparation is confirmed, expires after its 15-minute (900-second) window, or is cleared by a host restart.",
    context:
      "Here, rate_limited (ResourceExhausted) reflects the 16-item in-memory share preparation cap, not an AI model timeout or quota. No new preview or outbound intent was created, and existing unexpired preparations remain in memory.",
    recovery:
      "Confirm an existing prepared share preview if intended, wait for older 15-minute preparations to expire, or restart the host to clear in-memory preparations before clicking Prepare public share preview again.",
  },
  outbox_unavailable_reprepare_same_intent: {
    title: "Durable outbound share outbox was unavailable during confirmation",
    explanation:
      "The host verified and consumed the single-use share preparation, but its active VNextNetworkRuntime could not confirm durable outbox persistence for the outbound transfer intent.",
    context:
      "Here, dependency_unavailable (DependencyUnavailable) reflects local outbox storage availability, not an AI model or Base operation failure. The single-use preview has been cleared from this page; re-preparing the same saved artifact with the same recipient, selector and namespace reproduces the exact deterministic Intent ID and Public ObjectCID.",
    recovery:
      "Click Inspect share intent first to check whether the deterministic Intent ID was recorded, or click Prepare public share preview again with the same recipient, selector and namespace to create a fresh preview for the same deterministic Intent ID before confirming.",
  },
  outbox_unavailable: {
    title: "Durable outbound share outbox is currently unavailable",
    explanation:
      "The host's active VNextNetworkRuntime could not read the local durable outbound intent store while checking this Intent ID.",
    context:
      "Here, dependency_unavailable (DependencyUnavailable) reflects local outbox storage availability, not an AI model failure or a Base operation that uses Reconcile operation. No new outbound intent or network transfer was started by this status check.",
    recovery:
      "Verify that the host's network outbox storage is healthy and accessible (or restart the host if its local outbox handle is degraded), then click Inspect share intent again.",
  },
  public_store_unavailable: {
    title: "Local canonical public store is currently unavailable",
    explanation:
      "The host's active VNextNetworkRuntime could not read the canonical verified public store (vnext_verified.redb) while inspecting the requested Public ObjectCID.",
    context:
      "Here, dependency_unavailable (DependencyUnavailable) reflects local canonical public store availability, not an AI model failure or a Base operation that uses Reconcile operation. Inspecting a Public KU is a local read and does not modify private Vault artifacts.",
    recovery:
      "Verify that the host's canonical verified public store (vnext_verified.redb) is healthy and accessible (or restart the host if its storage handle is degraded), then click Inspect accepted Public KU again.",
  },
  provenance_unavailable: {
    title: "Accepted Public KU transport provenance is currently unavailable",
    explanation:
      "The host decoded the accepted Public KU from vnext_verified.redb, but its active VNextNetworkRuntime could not read the authenticated transport/selector provenance index for the requested selector CID.",
    context:
      "Here, dependency_unavailable (DependencyUnavailable) reflects local transport provenance store availability, not an AI model failure, original-source verification or a Base operation that uses Reconcile operation. Inspecting a Public KU does not expose private Vault artifacts.",
    recovery:
      "Verify the selector CID and that the host's network provenance store is healthy and accessible (or restart the host if its storage handle is degraded), then click Inspect accepted Public KU again.",
  },
  public_object_not_found: {
    title: "Accepted Public KU was not found in the local public store",
    explanation:
      "This host's canonical verified public store (vnext_verified.redb) does not contain an accepted Public KU with the requested Public ObjectCID. Peer reconciliation may still be pending, or the CID may refer to a private LOCAL_ONLY artifact or a different host.",
    context:
      "Inspecting a Public KU reads only already-accepted public records on this host; it does not fetch from a remote peer or expose private Vault artifacts.",
    recovery:
      "Verify that you entered the new Public ObjectCID (not the private ObjectCID), check Outbound share intent status on the sender until it reaches Acknowledged, and confirm the matching selector CID before clicking Inspect accepted Public KU again.",
  },
  intent_not_found: {
    title: "Outbound Public KU share intent was not found",
    explanation:
      "This host's durable outbox does not contain an outbound transfer intent with the requested Intent ID. Preparing a preview computes the deterministic Intent ID in memory (enqueued: false), but the durable intent is created only after Confirm public share succeeds on this sender host.",
    context:
      "No new outbound intent or network transfer was started by this status check.",
    recovery:
      "If you have only prepared a preview, complete Confirm public share first so the host enqueues the durable intent; otherwise verify that the 64-character hex Intent ID belongs to this sender host.",
  },
  explicit_public_confirmation_required: {
    title: "Explicit public confirmation is required before sharing",
    explanation:
      "The host rejected confirmation because confirm_public was not set to true. Public KU disclosure is a two-step boundary and never enqueues an outbound transfer intent without explicit public confirmation.",
    context:
      "No public object or outbound transfer intent was created, and any unexpired prepared share preview remains in memory.",
    recovery:
      "Review the prepared Public KU preview and permanent consequence, check Confirm public disclosure of this exact preview (confirm_public: true), and click Confirm public share.",
  },
  invalid_saved_ku: {
    title: "Saved private KU failed canonical share validation",
    explanation:
      "The host rejected share preparation because the requested private ObjectCID is not a 64-character lowercase hex digest or the saved artifact's canonical bytes and SemanticContentCID could not be verified as a canonical ObjectV1 manual knowledge kernel.",
    context:
      "No public preview, public object or outbound transfer intent was created, and existing saved knowledge remains unchanged.",
    recovery:
      "Use Search / list and Inspect to reload an accepted LOCAL_ONLY manual KU from the current host snapshot before clicking Prepare public share preview.",
  },
  invalid_public_ku: {
    title: "Public KU bytes failed canonical ObjectV1 validation",
    explanation:
      "The host could not decode the candidate or stored Public KU bytes as a canonical ObjectV1 knowledge kernel (ObjectKind(2) v1).",
    context:
      "Inspecting or preparing a Public KU does not modify or expose private LOCAL_ONLY Vault artifacts.",
    recovery:
      "Verify that the Public ObjectCID refers to a canonical D-045 Public KU (not a non-KU record or corrupted bytes) and try again.",
  },
  invalid_public_ku_intent: {
    title: "Outbound Public KU transfer intent could not be constructed",
    explanation:
      "The host compiled the candidate Public KU bytes, but OutboundTransferIntent validation rejected constructing the outbound Public transfer intent for the supplied peer, address, selector and namespace.",
    context:
      "No public preview, public object or outbound transfer intent was created, and the saved private artifact remains unchanged.",
    recovery:
      "Verify the recipient NodeID, loopback peer address (127.0.0.1:<port>), selector CID and namespace commitment, then click Prepare public share preview again.",
  },
  invalid_peer: {
    title: "Recipient NodeID is not a valid 32-byte hex identifier",
    explanation:
      "The host rejected share preparation because expected_node_id is not a valid 64-character hex NodeID.",
    context:
      "No public preview, public object or outbound transfer intent was created, and the saved private artifact remains unchanged.",
    recovery:
      "Enter the recipient peer's 64-character lowercase hex NodeID before clicking Prepare public share preview.",
  },
  invalid_peer_address: {
    title: "Peer loopback address is invalid",
    explanation:
      "The host rejected share preparation because the peer socket address could not be parsed as a valid loopback IP:port address.",
    context:
      "No public preview, public object or outbound transfer intent was created, and the saved private artifact remains unchanged.",
    recovery:
      "Enter a valid loopback peer address in 127.0.0.1:<port> format before clicking Prepare public share preview.",
  },
  invalid_selector: {
    title: "Selector CID is not a valid 32-byte hex digest",
    explanation:
      "The host rejected the request because selector is not a valid 64-character hex SelectorCid.",
    context:
      "No public preview, outbound transfer intent or private Vault mutation was performed.",
    recovery:
      "Enter a 64-character lowercase hex Selector CID before retrying Prepare public share preview or Inspect accepted Public KU.",
  },
  invalid_namespace: {
    title: "Namespace commitment is not a valid 32-byte hex digest",
    explanation:
      "The host rejected share preparation because namespace is not a valid 64-character hex NamespaceCommitment.",
    context:
      "No public preview, public object or outbound transfer intent was created, and the saved private artifact remains unchanged.",
    recovery:
      "Enter a 64-character lowercase hex Namespace commitment before clicking Prepare public share preview.",
  },
  invalid_public_identity: {
    title: "Accepted Public ObjectCID is not a valid 32-byte hex digest",
    explanation:
      "The host rejected Public KU inspection because object_cid is not a valid 64-character hex ObjectCid.",
    context:
      "Inspecting a Public KU is a local read and does not modify or expose private Vault artifacts.",
    recovery:
      "Enter the 64-character lowercase hex Public ObjectCID before clicking Inspect accepted Public KU.",
  },
  invalid_intent_identity: {
    title: "Outbound share Intent ID is not a valid 32-byte hex digest",
    explanation:
      "The host rejected outbound intent inspection because intent_id is not a valid 64-character hex identifier.",
    context:
      "No new outbound intent or network transfer was started by this status check.",
    recovery:
      "Enter the 64-character lowercase hex Intent ID returned by Prepare public share preview or Confirm public share before clicking Inspect share intent.",
  },
  consent_randomness_unavailable: {
    title: "Host randomness for share preparation is currently unavailable",
    explanation:
      "The host validated the Public KU candidate, but OS randomness (getrandom) could not generate the single-use 32-byte prepared_id capability token.",
    context:
      "Here, dependency_unavailable (DependencyUnavailable) reflects host OS entropy availability, not an AI model failure or a Base operation that uses Reconcile operation. No public preview or outbound transfer intent was created.",
    recovery:
      "Verify host OS entropy availability (or restart the host if the system RNG is unhealthy), then click Prepare public share preview again.",
  },
  invalid_clock: {
    title: "Host system clock is invalid for Public KU share expiry",
    explanation:
      "The host rejected the Public KU share action because its system clock is before the Unix epoch or overflowed while computing the 15-minute (900-second) preparation expiry window.",
    context:
      "Here, dependency_unavailable (DependencyUnavailable) reflects the host system clock check, not an AI model failure or a Base operation that uses Reconcile operation. No outbound transfer intent was enqueued, and any unexpired prepared share preview remains in memory.",
    recovery:
      "Correct the host's system clock to current UTC time, then retry Prepare public share preview or Confirm public share.",
  },
};

export function describeKuError(error: unknown) {
  const failure = error instanceof KuError ? error.failure : undefined;
  const reasons = failure?.limitations ?? [];
  const deadline =
    failure?.code === "ResourceExhausted" && reasons.includes("deadline");
  const diagnostics = reasons.filter(
    (reason) =>
      reason.startsWith("schema: ") || reason.startsWith("grounding: "),
  );
  const conceptLabel = reasons.includes("concept_label");
  const duplicateId = reasons.includes("duplicate_id");
  const shareNetworkUnavailable =
    failure?.code === "CapabilityDisabled" &&
    reasons.includes("ku_public_share_network_unavailable");
  const shareFeedback = shareNetworkUnavailable
    ? ({
        title: "Public KU share network runtime is disabled",
        explanation:
          "The host keeps networking disabled by default. Confirming a Public KU share or inspecting outbound/accepted peer share state requires an active opt-in VNextNetworkRuntime attached to this host.",
        context:
          "No payload was enqueued or sent. Any unexpired prepared share preview remains available in memory.",
      } satisfies ShareErrorFeedback)
    : reasons
        .map((reason) => SHARE_ERROR_FEEDBACK[reason])
        .find((item): item is ShareErrorFeedback => item !== undefined);
  const invalidCandidate =
    diagnostics.length > 0 ||
    reasons.includes("oneof") ||
    conceptLabel ||
    duplicateId;
  const technical = [
    error instanceof Error ? error.message : "Local operation failed",
    failure?.code,
    ...reasons,
    failure
      ? `Retryable: ${failure.retryable}; reconcile before retry: ${failure.reconcile_before_retry}`
      : "",
  ]
    .filter(Boolean)
    .join(" · ");
  return {
    title: deadline
      ? "Encoding timed out"
      : shareFeedback
        ? shareFeedback.title
        : duplicateId
          ? "AI output repeats an internal identifier"
          : conceptLabel
            ? "AI concept label differs from its source evidence"
            : invalidCandidate
              ? "AI output does not match the KU schema"
              : "Local operation could not finish",
    explanation: deadline
      ? "The host exhausted the encoding time budget before returning a validated preview. The current experimental Ollama workflow has a 600-second (10-minute) budget, including worker startup, model inference, any repair call and validation."
      : shareFeedback
        ? shareFeedback.explanation
        : duplicateId
          ? "The model reused an identifier within a collection of concepts, statements or coverage entries. Distinct items need distinct keys; references must point to the intended item. Each required unit needs one coverage entry. No validated preview was produced."
          : conceptLabel
            ? "A concept.label returned by the model does not exactly match its evidence.quote. The label must copy the source wording, including case, accents and spelling. No validated preview was produced. This error does not establish that the concept is missing from the Registry or that your sentence is factually wrong."
            : invalidCandidate
              ? "The host rejected the model's structured output. No validated preview was produced. The schema issues below identify fields to repair; they do not judge whether your source is factually true."
              : "The host did not return a successful result for this request. Read the technical reason below before deciding whether to retry.",
    context: deadline
      ? "Here, rate_limited is the API category for ResourceExhausted; deadline identifies a time limit. It does not indicate an AI account quota, prove that RAM ran out, or mean your sentence is incorrect. The reply does not identify the exact internal step that timed out."
      : shareFeedback
        ? shareFeedback.context
        : invalidCandidate
          ? "DependencyUnavailable is the transport category for this failure; it does not mean Ollama is disconnected. The automatic repair allowance is bounded; checking the recorded outcome will not run another repair."
          : "An interrupted response does not establish whether a write completed. Existing saved knowledge remains separate from this request.",
    recovery:
      shareFeedback?.recovery ??
      (shareFeedback
        ? "Check the technical reason and refresh host status. Correct the reported problem before retrying the Public KU share action."
        : failure?.reconcile_before_retry ||
            (error instanceof KuError && error.uncertain)
          ? "Use Reconcile operation to read the host's recorded outcome. It does not rerun AI. After a failed or canceled outcome, start a new attempt. If work remains pending, cancel it before starting again."
          : "Check the technical reason and refresh host status. Correct the reported problem before starting another request."),
    technical,
    diagnostics,
    // D-045 share failures never describe the Base private draft operation, so
    // callers must not mark Base work uncertain or offer Base reconciliation.
    isShareError: shareFeedback !== undefined,
  };
}
