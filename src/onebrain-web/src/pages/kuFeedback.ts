import { KuError } from "../api/ku";

export function isStaleSharePreparationError(error: unknown): boolean {
  const reasons =
    error instanceof KuError ? (error.failure?.limitations ?? []) : [];
  return (
    reasons.includes("prepared_share_missing") ||
    reasons.includes("public_ku_consent_expired")
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
  const staleSharePreparation = isStaleSharePreparationError(error);
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
    title: deadline ? "Encoding timed out" : shareNetworkUnavailable ? "Public KU share network runtime is disabled" : staleSharePreparation ? "Prepared Public KU share preview is no longer valid" : duplicateId ? "AI output repeats an internal identifier" : conceptLabel ? "AI concept label differs from its source evidence" : invalidCandidate ? "AI output does not match the KU schema" : "Local operation could not finish",
    explanation: deadline
      ? "The host exhausted the encoding time budget before returning a validated preview. The current experimental Ollama workflow has a 600-second (10-minute) budget, including worker startup, model inference, any repair call and validation."
      : shareNetworkUnavailable ? "The host keeps networking disabled by default. Confirming a Public KU share or inspecting outbound/accepted peer share state requires an active opt-in VNextNetworkRuntime attached to this host."
      : staleSharePreparation ? "The host rejected confirmation because this single-use in-memory share preparation expired after its 15-minute (900-second) window, was evicted under the 16-item in-memory preparation limit, was already used once, or was cleared when the host restarted."
      : duplicateId ? "The model reused an identifier within a collection of concepts, statements or coverage entries. Distinct items need distinct keys; references must point to the intended item. Each required unit needs one coverage entry. No validated preview was produced."
      : conceptLabel ? "A concept.label returned by the model does not exactly match its evidence.quote. The label must copy the source wording, including case, accents and spelling. No validated preview was produced. This error does not establish that the concept is missing from the Registry or that your sentence is factually wrong."
      : invalidCandidate ? "The host rejected the model's structured output. No validated preview was produced. The schema issues below identify fields to repair; they do not judge whether your source is factually true."
      : "The host did not return a successful result for this request. Read the technical reason below before deciding whether to retry.",
    context: deadline
      ? "Here, rate_limited is the API category for ResourceExhausted; deadline identifies a time limit. It does not indicate an AI account quota, prove that RAM ran out, or mean your sentence is incorrect. The reply does not identify the exact internal step that timed out."
      : shareNetworkUnavailable ? "No payload was enqueued or sent. Any unexpired prepared share preview remains available in memory."
      : staleSharePreparation ? "No payload was enqueued or sent by this failed confirmation, and the saved private artifact remains unchanged. The stale preview has been cleared from this page."
      : invalidCandidate ? "DependencyUnavailable is the transport category for this failure; it does not mean Ollama is disconnected. The automatic repair allowance is bounded; checking the recorded outcome will not run another repair."
      : "An interrupted response does not establish whether a write completed. Existing saved knowledge remains separate from this request.",
    recovery: failure?.reconcile_before_retry || (error instanceof KuError && error.uncertain)
      ? "Use Reconcile operation to read the host's recorded outcome. It does not rerun AI. After a failed or canceled outcome, start a new attempt. If work remains pending, cancel it before starting again."
      : staleSharePreparation
        ? "Click Prepare public share preview again to create and review a fresh preview before confirming public disclosure."
        : "Check the technical reason and refresh host status. Correct the reported problem before starting another request.",
    technical,
    diagnostics,
  };
}
