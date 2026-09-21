import { KuError } from "../api/ku";

export function describeKuError(error: unknown) {
  const failure = error instanceof KuError ? error.failure : undefined;
  const reasons = failure?.limitations ?? [];
  const deadline = failure?.code === "ResourceExhausted" && reasons.includes("deadline");
  const diagnostics = reasons.filter(reason => reason.startsWith("schema: ") || reason.startsWith("grounding: "));
  const conceptLabel = reasons.includes("concept_label");
  const duplicateId = reasons.includes("duplicate_id");
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
    title: deadline ? "Encoding timed out" : duplicateId ? "AI output repeats an internal identifier" : conceptLabel ? "AI concept label differs from its source evidence" : invalidCandidate ? "AI output does not match the KU schema" : "Local operation could not finish",
    explanation: deadline
      ? "The host exhausted the encoding time budget before returning a validated preview. The current experimental Ollama workflow has a 600-second (10-minute) budget, including worker startup, model inference, any repair call and validation."
      : duplicateId ? "The model reused an identifier within a collection of concepts, statements or coverage entries. Distinct items need distinct keys; references must point to the intended item. Each required unit needs one coverage entry. No validated preview was produced."
      : conceptLabel ? "A concept.label returned by the model does not exactly match its evidence.quote. The label must copy the source wording, including case, accents and spelling. No validated preview was produced. This error does not establish that the concept is missing from the Registry or that your sentence is factually wrong."
      : invalidCandidate ? "The host rejected the model's structured output. No validated preview was produced. The schema issues below identify fields to repair; they do not judge whether your source is factually true."
      : "The host did not return a successful result for this request. Read the technical reason below before deciding whether to retry.",
    context: deadline
      ? "Here, rate_limited is the API category for ResourceExhausted; deadline identifies a time limit. It does not indicate an AI account quota, prove that RAM ran out, or mean your sentence is incorrect. The reply does not identify the exact internal step that timed out."
      : invalidCandidate ? "DependencyUnavailable is the transport category for this failure; it does not mean Ollama is disconnected. The automatic repair allowance is bounded; checking the recorded outcome will not run another repair."
      : "An interrupted response does not establish whether a write completed. Existing saved knowledge remains separate from this request.",
    recovery: failure?.reconcile_before_retry || (error instanceof KuError && error.uncertain)
      ? "Use Reconcile operation to read the host's recorded outcome. It does not rerun AI. After a failed or canceled outcome, start a new attempt. If work remains pending, cancel it before starting again."
      : "Check the technical reason and refresh host status. Correct the reported problem before starting another request.",
    technical,
    diagnostics,
  };
}
