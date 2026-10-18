import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { KuWorkflowPage } from "../src/pages/KuWorkflow";
import { createKuClient } from "../src/api/ku";
import axe from "axe-core";
import userEvent from "@testing-library/user-event";

afterEach(cleanup);
const op = "1".repeat(64),
  object = "2".repeat(64),
  semantic = "3".repeat(64),
  source = "4".repeat(64),
  ccid = "5".repeat(32);
const session = {
  process_generation: "6".repeat(64),
  dataset_generation: "7".repeat(64),
};
const meta = {
  lifecycle: "active",
  coverage: "local_only",
  limitations: ["real_model_unqualified"],
  continuation: null,
};
const summary = {
  object_cid: object,
  semantic_content_cid: semantic,
  disclosure_class: "LOCAL_ONLY",
  artifact_validity: "accepted_known",
  coverage: "local_only",
  limitations: [],
  executable: false,
};
function fixture(
  options: {
    unresolved?: boolean;
    loseSave?: boolean;
    noEditor?: boolean;
    emptyCatalog?: boolean;
    reserveLoss?: boolean;
    ai?: boolean;
    prepareWait?: Promise<void>;
    prepareFailure?: "deadline" | "resource" | "oneof" | "concept_label" | "duplicate_id";
    reservedAfterFailure?: boolean;
    shareNetworkDisabled?: boolean;
    sharePrepareFailure?:
      | "unsupported_public_ku"
      | "prepared_limit"
      | "saved_ku_not_accessible";
    shareConfirmFailure?:
      | "prepared_share_missing"
      | "public_ku_consent_expired"
      | "saved_access_changed"
      | "outbox_unavailable_reprepare_same_intent";
    shareIntentFailure?: "intent_not_found" | "outbox_unavailable";
    shareInspectFailure?:
      | "public_object_not_found"
      | "public_store_unavailable"
      | "provenance_unavailable";
  } = {},
) {
  let saved = false;
  let failed = false;
  let sharePrepareCount = 0;
  let currentOp = op;
  let currentSession = { ...session };
  const publicObject = "d".repeat(64),
    preparedShareId = "e".repeat(64),
    freshPreparedShareId = "a".repeat(64),
    shareIntent = "f".repeat(64);
  const calls: { path: string; body: any; init: RequestInit }[] = [];
  const prepared = () => ({
    operation_id: currentOp,
    validity: options.unresolved ? "needs_resolution" : "ready",
    object_cids: options.unresolved ? [] : [object],
    artifacts: options.unresolved
      ? []
      : [
          {
            object_cid: object,
            semantic_content_cid: semantic,
            canonical_preview: "AQ==",
          },
        ],
    limitations: options.unresolved ? ["needs_resolution"] : [],
    destination: "LOCAL_ONLY",
    executable: false,
    registry_release_root: "8".repeat(64),
    semantic_profile: "ku-semantic-content/1.0",
  });
  const receipt = () => ({
    operation_id: currentOp,
    state: saved ? "committed" : failed ? options.reservedAfterFailure ? "reserved" : "failed" : "prepared",
    object_cids: saved ? [object] : [],
    limitations: [],
    published: false,
    authorizes_reward: false,
  });
  const fetcher = vi.fn(
    async (url: string | URL | Request, init?: RequestInit) => {
      const path = String(url);
      const body = init?.body ? JSON.parse(String(init.body)) : undefined;
      calls.push({ path, body, init: init! });
      let payload: unknown;
      if (path.endsWith("/status"))
        payload = {
          lifecycle: "active",
          coverage: "local_only",
          limitations: [],
          registry_ready: true,
          local_encoder_ready: true,
          remote_encoding_enabled: false,
          direct_issuance_enabled: false,
        };
      else if (path.endsWith("/reservations")) {
        if (options.reserveLoss) {
          options.reserveLoss = false;
          throw new TypeError("network lost");
        }
        payload = { operation_id: currentOp };
      } else if (path.endsWith("/editor")) {
        if (options.noEditor)
          return new Response(
            JSON.stringify({
              ok: false,
              error: {
                code: "dependency_unavailable",
                failure: {
                  code: "DependencyUnavailable",
                  retryable: true,
                  reconcile_before_retry: false,
                  limitations: [],
                },
              },
            }),
            { status: 503 },
          );
        switch (body.request.action) {
          case "models":
            payload = {
              models: options.ai
                ? [
                    {
                      model: "qwen3:8b",
                      implementation_commitment: "9".repeat(64),
                      experimental: true,
                    },
                  ]
                : [],
              limitations: [],
              consent_text:
                "I consent to local experimental encoding and private retention.",
            };
            break;
          case "encode_text":
            payload = {
              operation_id: currentOp,
              idempotency_key: currentOp,
              source_refs: [source],
              input_mode: "local_ai",
              registry_release_root: "8".repeat(64),
              implementation_commitment: "9".repeat(64),
              semantic_profile: "ku-semantic-content/1.0",
              destination: "LOCAL_ONLY",
            };
            break;
          case "catalog":
            payload = {
              sources: options.emptyCatalog ? [] : [{ source_ref: source, label: "Host source" }],
              limitations: [],
            };
            break;
          case "resolve":
            payload = {
              candidates: options.unresolved ? [] : [{ ccid }],
              limitations: [],
            };
            break;
          case "draft":
            payload = {
              ...body.request.payload,
              source_refs: [source],
              input_mode: "resolved_semantic_draft",
              draft_ref: source,
              registry_release_root: "8".repeat(64),
              implementation_commitment: "9".repeat(64),
              semantic_profile: "ku-semantic-content/1.0",
              destination: "LOCAL_ONLY",
            };
            break;
          case "share_prepare":
            if (options.sharePrepareFailure)
              return new Response(
                JSON.stringify({
                  ok: false,
                  error: {
                    code:
                      options.sharePrepareFailure === "unsupported_public_ku"
                        ? "invalid_request"
                        : options.sharePrepareFailure ===
                            "saved_ku_not_accessible"
                          ? "not_found"
                          : "rate_limited",
                    failure: {
                      code:
                        options.sharePrepareFailure === "unsupported_public_ku"
                          ? "InvalidRequest"
                          : options.sharePrepareFailure ===
                              "saved_ku_not_accessible"
                            ? "NotFound"
                            : "ResourceExhausted",
                      retryable:
                        options.sharePrepareFailure === "prepared_limit",
                      reconcile_before_retry:
                        options.sharePrepareFailure === "prepared_limit",
                      limitations: [options.sharePrepareFailure],
                    },
                  },
                }),
                {
                  status:
                    options.sharePrepareFailure === "unsupported_public_ku"
                      ? 400
                      : options.sharePrepareFailure ===
                          "saved_ku_not_accessible"
                        ? 404
                        : 429,
                },
              );
            payload = {
              event: "share_prepared",
              prepared_id:
                sharePrepareCount++ === 0
                  ? preparedShareId
                  : freshPreparedShareId,
              intent_id: shareIntent,
              public_object_cid: publicObject,
              semantic_content_cid: semantic,
              canonical_base64: "Ag==",
              semantic: {
                semantic_content_cid: semantic,
                statements: [
                  {
                    predicate_ccid: ccid,
                    text_arguments: ["Water is a liquid."],
                  },
                ],
                fidelity: "unassessed",
              },
              expected_node_id: body.request.payload.expected_node_id,
              address: body.request.payload.address,
              selector: body.request.payload.selector,
              namespace: body.request.payload.namespace,
              expires_at: 1900000900,
              consequence:
                "This new object and its semantic content become Public. The recipient may retain and forward it permanently. The private source and original private object are not included. Provenance is authenticated transport only, not original-source, author or fidelity verification.",
              enqueued: false,
              payload_sent: false,
              support: "one predicate and one text literal only",
            };
            break;
          case "share_confirm":
            if (options.shareNetworkDisabled)
              return new Response(
                JSON.stringify({
                  ok: false,
                  error: {
                    code: "capability_disabled",
                    failure: {
                      code: "CapabilityDisabled",
                      retryable: false,
                      reconcile_before_retry: false,
                      limitations: ["ku_public_share_network_unavailable"],
                    },
                  },
                }),
                { status: 503 },
              );
            if (options.shareConfirmFailure)
              return new Response(
                JSON.stringify({
                  ok: false,
                  error: {
                    code:
                      options.shareConfirmFailure === "prepared_share_missing"
                        ? "not_found"
                        : options.shareConfirmFailure === "saved_access_changed"
                          ? "conflict"
                          : options.shareConfirmFailure ===
                              "outbox_unavailable_reprepare_same_intent"
                            ? "dependency_unavailable"
                            : "expired",
                    failure: {
                      code:
                        options.shareConfirmFailure === "prepared_share_missing"
                          ? "NotFound"
                          : options.shareConfirmFailure ===
                              "saved_access_changed"
                            ? "Conflict"
                            : options.shareConfirmFailure ===
                                "outbox_unavailable_reprepare_same_intent"
                              ? "DependencyUnavailable"
                              : "Expired",
                      retryable:
                        options.shareConfirmFailure ===
                        "outbox_unavailable_reprepare_same_intent",
                      reconcile_before_retry:
                        options.shareConfirmFailure ===
                        "outbox_unavailable_reprepare_same_intent",
                      limitations: [options.shareConfirmFailure],
                    },
                  },
                }),
                {
                  status:
                    options.shareConfirmFailure === "prepared_share_missing"
                      ? 404
                      : options.shareConfirmFailure === "saved_access_changed"
                        ? 409
                        : options.shareConfirmFailure ===
                            "outbox_unavailable_reprepare_same_intent"
                          ? 503
                          : 410,
                },
              );
            payload = {
              event: "share_confirmed",
              intent_id: shareIntent,
              public_object_cid: publicObject,
              outbox: "Enqueued",
              delivery: "pending; inspect intent for acknowledgement",
              authorizes_reward: false,
            };
            break;
          case "share_intent":
            if (options.shareIntentFailure)
              return new Response(
                JSON.stringify({
                  ok: false,
                  error: {
                    code:
                      options.shareIntentFailure === "intent_not_found"
                        ? "not_found"
                        : "dependency_unavailable",
                    failure: {
                      code:
                        options.shareIntentFailure === "intent_not_found"
                          ? "NotFound"
                          : "DependencyUnavailable",
                      retryable:
                        options.shareIntentFailure === "outbox_unavailable",
                      reconcile_before_retry:
                        options.shareIntentFailure === "outbox_unavailable",
                      limitations: [options.shareIntentFailure],
                    },
                  },
                }),
                {
                  status:
                    options.shareIntentFailure === "intent_not_found"
                      ? 404
                      : 503,
                },
              );
            payload = {
              event: "intent",
              intent_id: body.request.payload.intent_id,
              public_object_cid: publicObject,
              state: "Acknowledged",
              terminal_sequence: 1,
              claims_network_completion: false,
            };
            break;
          case "share_inspect":
            if (options.shareInspectFailure)
              return new Response(
                JSON.stringify({
                  ok: false,
                  error: {
                    code:
                      options.shareInspectFailure === "public_object_not_found"
                        ? "not_found"
                        : "dependency_unavailable",
                    failure: {
                      code:
                        options.shareInspectFailure ===
                        "public_object_not_found"
                          ? "NotFound"
                          : "DependencyUnavailable",
                      retryable:
                        options.shareInspectFailure !==
                        "public_object_not_found",
                      reconcile_before_retry:
                        options.shareInspectFailure !==
                        "public_object_not_found",
                      limitations: [options.shareInspectFailure],
                    },
                  },
                }),
                {
                  status:
                    options.shareInspectFailure === "public_object_not_found"
                      ? 404
                      : 503,
                },
              );
            payload = {
              event: "public_object",
              object_cid: body.request.payload.object_cid,
              canonical_base64: "Ag==",
              semantic: {
                semantic_content_cid: semantic,
                statements: [
                  {
                    predicate_ccid: ccid,
                    text_arguments: ["Water is a liquid."],
                  },
                ],
                fidelity: "unassessed",
              },
              authenticated_source_peers: ["12".repeat(32)],
              provenance:
                "transport/selector only; original source withheld and unverifiable",
              fidelity: "unassessed",
              authorizes_reward: false,
            };
            break;
        }
      } else {
        switch (body.request.operation) {
          case "prepare":
            if (options.prepareWait) await options.prepareWait;
            if (options.prepareFailure) {
              failed = true;
              return new Response(JSON.stringify({
                ok: false,
                error: { code: "rate_limited", failure: {
                  code: ["oneof", "concept_label", "duplicate_id"].includes(options.prepareFailure) ? "DependencyUnavailable" : "ResourceExhausted", retryable: true,
                  reconcile_before_retry: true, limitations: options.prepareFailure === "oneof"
                    ? ["oneof", "schema: $.statements[0].arguments[0].unit: missing_field; include this required field"]
                    : [options.prepareFailure],
                } },
              }), { status: 429 });
            }
            payload = prepared();
            break;
          case "revise":
          case "preview":
            payload = prepared();
            break;
          case "save":
            saved = true;
            if (options.loseSave) {
              options.loseSave = false;
              throw new TypeError("reply lost");
            }
            payload = receipt();
            break;
          case "reconcile":
            payload = receipt();
            break;
          case "status":
            payload = {
              lifecycle: "active",
              receipt: { ...receipt(), state: "confirming" },
            };
            break;
          case "list":
          case "search":
            payload = {
              items: saved || options.noEditor || options.emptyCatalog ? [summary] : [],
              coverage: "local_only",
              snapshot_frontier: "a".repeat(64),
              limitations: ["authorized_snapshot_only"],
            };
            break;
          case "get":
            payload = { ...summary, canonical_bytes: "AQ==" };
            break;
          case "cancel":
            payload = { ...receipt(), state: "canceled" };
            currentOp = "b".repeat(64);
            break;
          default:
            throw new Error("unexpected operation");
        }
      }
      return new Response(
        JSON.stringify({
          ok: true,
          data: { session: currentSession, payload, model_qualified: false },
          meta,
        }),
      );
    },
  );
  return {
    client: createKuClient(
      async () => ({ baseUrl: "http://local.test", token: "private-token" }),
      fetcher as typeof fetch,
    ),
    calls,
    rotate: () => {
      currentSession = { ...session, process_generation: "c".repeat(64) };
    },
  };
}
async function fill() {
  await screen.findByRole("option", { name: "Host source" });
  fireEvent.change(screen.getByLabelText("Admitted source"), {
    target: { value: source },
  });
  fireEvent.change(screen.getByLabelText("Predicate label"), {
    target: { value: "water" },
  });
  fireEvent.change(screen.getByLabelText("Text argument (manual assertion)"), {
    target: { value: "Private manually asserted text" },
  });
}
async function preview() {
  fireEvent.click(screen.getByRole("button", { name: "Preview and validate" }));
  await screen.findByRole("heading", { name: "Exact prepared preview" });
  await waitFor(() =>
    expect(
      (
        screen.getByRole("button", {
          name: "Reconcile operation",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false),
  );
}
describe("local KU component journey", () => {
  it("guides empty-catalog setup while keeping saved reads and consented AI separate", async () => {
    const options = { emptyCatalog: true, ai: true };
    const f = fixture(options);
    render(<KuWorkflowPage client={f.client} />);
    await screen.findByText(/No manual sources available/);
    expect(screen.getByRole("link", { name: "manual source setup guide" }).getAttribute("href"))
      .toContain("#provision-a-developer-owned-manual-source");
    expect(screen.getByLabelText("Admitted source").matches(":disabled")).toBe(true);
    expect(screen.getByRole("button", { name: "Preview and validate" }).matches(":disabled")).toBe(true);
    expect(screen.getByLabelText("Source text to encode").matches(":disabled")).toBe(false);
    expect(screen.getByRole("button", { name: "Encode and preview" }).matches(":disabled")).toBe(true);
    fireEvent.change(screen.getByLabelText("Source text to encode"), { target: { value: "Developer-owned text" } });
    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByRole("button", { name: "Encode and preview" }).matches(":disabled")).toBe(false);
    fireEvent.click(await screen.findByRole("button", { name: `Inspect ${object}` }));
    await screen.findByRole("heading", { name: "Inspect saved artifact" });
    expect(screen.getByText("AQ==").textContent).toBe("AQ==");
    fireEvent.click(screen.getByRole("button", { name: "Refresh host status" }));
    await waitFor(() => expect(f.calls.filter(c => c.body?.request?.action === "catalog")).toHaveLength(2));
    expect(screen.getByRole("link", { name: "manual source setup guide" })).toBeTruthy();
    await waitFor(() => expect(screen.getByRole("button", { name: "Refresh host status" }).matches(":disabled")).toBe(false));
    options.emptyCatalog = false; // Simulate explicit admission and host restart.
    fireEvent.click(screen.getByRole("button", { name: "Refresh host status" }));
    await screen.findByRole("option", { name: "Host source" });
    expect(screen.queryByText(/No manual sources available/)).toBeNull();
    expect(screen.getByLabelText("Admitted source").matches(":disabled")).toBe(false);
    expect(f.calls.filter(c => c.body?.request?.operation && c.body.request.operation !== "list" && c.body.request.operation !== "get")).toHaveLength(0);
    expect(f.calls.filter(c => c.path.endsWith("/reservations") || ["draft", "encode_text", "review_start"].includes(c.body?.request?.action))).toHaveLength(0);
  });
  it("explains duplicate internal identifiers without implying a transport outage", async () => {
    const f = fixture({ ai: true, prepareFailure: "duplicate_id" });
    render(<KuWorkflowPage client={f.client} />);
    await screen.findByRole("option", { name: "qwen3:8b — experimental" });
    fireEvent.change(screen.getByLabelText("Source text to encode"), { target: { value: "Water boils." } });
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Encode and preview" }));
    await screen.findByRole("heading", { name: "AI output repeats an internal identifier" });
    expect(screen.getByRole("alert").textContent).toContain("Distinct items need distinct keys");
    expect(screen.queryByRole("heading", { name: "Exact prepared preview" })).toBeNull();
  });
  it("explains concept label failures even when an older host has no field diagnostics", async () => {
    const f = fixture({ ai: true, prepareFailure: "concept_label" });
    render(<KuWorkflowPage client={f.client} />);
    await screen.findByRole("option", { name: "qwen3:8b — experimental" });
    fireEvent.change(screen.getByLabelText("Source text to encode"), { target: { value: "Water boils." } });
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Encode and preview" }));
    await screen.findByRole("heading", { name: "AI concept label differs from its source evidence" });
    expect(screen.getByRole("alert").textContent).toContain("does not establish that the concept is missing from the Registry");
    expect(screen.getByRole("alert").textContent).toContain("does not mean Ollama is disconnected");
    expect(screen.queryByRole("heading", { name: "Exact prepared preview" })).toBeNull();
  });
  it("shows field-level schema diagnostics without opening technical details", async () => {
    const f = fixture({ ai: true, prepareFailure: "oneof" });
    render(<KuWorkflowPage client={f.client} />);
    await screen.findByRole("option", { name: "qwen3:8b — experimental" });
    fireEvent.change(screen.getByLabelText("Source text to encode"), { target: { value: "Water boils." } });
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Encode and preview" }));
    await screen.findByRole("heading", { name: "AI output does not match the KU schema" });
    expect(screen.getByRole("region", { name: "Schema validation issues" }).textContent).toContain("$.statements[0].arguments[0].unit: missing_field");
    expect(screen.queryByRole("heading", { name: "Exact prepared preview" })).toBeNull();
    expect(f.calls.filter(c => c.body?.request?.operation === "save")).toHaveLength(0);
  });
  it("keeps the original failure visible and explains a reserved reconciliation without replay", async () => {
    const f = fixture({ ai: true, prepareFailure: "resource", reservedAfterFailure: true });
    render(<KuWorkflowPage client={f.client} />);
    await screen.findByRole("option", { name: "qwen3:8b — experimental" });
    fireEvent.change(screen.getByLabelText("Source text to encode"), { target: { value: "Water boils." } });
    fireEvent.click(screen.getByRole("checkbox"));
    const encode = screen.getByRole("button", { name: "Encode and preview" }) as HTMLButtonElement;
    fireEvent.click(encode);
    await screen.findByRole("heading", { name: "Local operation could not finish" });
    fireEvent.click(screen.getByRole("button", { name: "Check recorded outcome" }));
    await screen.findByRole("heading", { name: "Recorded outcome: reserved" });
    expect(screen.getByRole("alert").textContent).toContain("ResourceExhausted");
    expect(screen.getByRole("status").textContent).toContain("does not prove that AI is still running");
    expect(encode.matches(":disabled")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Cancel reservation and unlock editor" }));
    await screen.findByRole("heading", { name: "Recorded outcome: canceled" });
    await waitFor(() => expect(encode.disabled).toBe(false));
    expect(screen.getByLabelText("Source text to encode").getAttribute("disabled")).toBeNull();
    expect(f.calls.filter(c => c.body?.request?.operation === "prepare")).toHaveLength(1);
    expect(f.calls.filter(c => c.body?.request?.operation === "save")).toHaveLength(0);
  });
  it.each(["deadline", "resource"] as const)(
    "explains %s without replaying inference and unlocks only after reconciliation",
    async (reason) => {
      const f = fixture({ ai: true, prepareFailure: reason });
      render(<KuWorkflowPage client={f.client} />);
      await screen.findByRole("option", { name: "qwen3:8b — experimental" });
      fireEvent.change(screen.getByLabelText("Source text to encode"), {
        target: { value: "Water boils." },
      });
      fireEvent.click(screen.getByRole("checkbox"));
      const encode = screen.getByRole("button", { name: "Encode and preview" }) as HTMLButtonElement;
      fireEvent.click(encode);
      await screen.findByRole("heading", { name: reason === "deadline"
        ? "Encoding timed out" : "Local operation could not finish" });
      const alert = screen.getByRole("alert");
      expect(alert.textContent).toContain(reason);
      if (reason === "deadline") expect(alert.textContent).toContain("600-second (10-minute)");
      else expect(alert.textContent).not.toContain("600-second (10-minute)");
      expect(alert.textContent).toContain("Source accepted");
      expect(encode.disabled).toBe(true);
      expect(screen.queryByRole("heading", { name: "Exact prepared preview" })).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Check recorded outcome" }));
      await waitFor(() => expect(encode.disabled).toBe(false));
      expect(screen.getByRole("status").textContent).toContain("failed");
      expect(f.calls.filter(c => c.body?.request?.action === "encode_text")).toHaveLength(1);
      expect(f.calls.filter(c => c.body?.request?.operation === "prepare")).toHaveLength(1);
      expect(f.calls.filter(c => c.body?.request?.operation === "save")).toHaveLength(0);
    },
  );
  it("requires consent, selects qwen3 and saves only after AI preview", async () => {
    const f = fixture({ ai: true });
    render(<KuWorkflowPage client={f.client} />);
    await screen.findByRole("option", { name: "qwen3:8b — experimental" });
    const encode = screen.getByRole("button", {
      name: "Encode and preview",
    }) as HTMLButtonElement;
    fireEvent.change(screen.getByLabelText("Source text to encode"), {
      target: { value: "Copper is conductive." },
    });
    expect(encode.disabled).toBe(true);
    fireEvent.click(screen.getByRole("checkbox"));
    expect(encode.disabled).toBe(false);
    fireEvent.click(encode);
    await screen.findByRole("heading", { name: "Exact prepared preview" });
    const intake = f.calls.find(
      (c) => c.body?.request?.action === "encode_text",
    )!;
    expect(intake.body.request.payload).toEqual({
      operation_id: op,
      idempotency_key: op,
      model: "qwen3:8b",
      text: "Copper is conductive.",
      consent: true,
    });
    expect(f.calls.some((c) => c.body?.request?.operation === "save")).toBe(
      false,
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Save exact preview privately",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Save exact preview privately" }),
    );
    await waitFor(() =>
      expect(f.calls.some((c) => c.body?.request?.operation === "save")).toBe(
        true,
      ),
    );
  });
  it("can cancel a running AI call and discards its late preview", async () => {
    let release!: () => void;
    const f = fixture({
      ai: true,
      prepareWait: new Promise<void>((resolve) => {
        release = resolve;
      }),
    });
    render(<KuWorkflowPage client={f.client} />);
    await screen.findByRole("option", { name: "qwen3:8b — experimental" });
    fireEvent.change(screen.getByLabelText("Source text to encode"), {
      target: { value: "Copper is conductive." },
    });
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Encode and preview" }));
    await waitFor(() =>
      expect(
        f.calls.some((c) => c.body?.request?.operation === "prepare"),
      ).toBe(true),
    );
    const cancel = screen.getByRole("button", {
      name: "Cancel pending draft",
    }) as HTMLButtonElement;
    expect(cancel.disabled).toBe(false);
    fireEvent.click(cancel);
    await waitFor(() =>
      expect(f.calls.some((c) => c.body?.request?.operation === "cancel")).toBe(
        true,
      ),
    );
    release();
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("canceled"),
    );
    expect(
      screen.queryByRole("heading", { name: "Exact prepared preview" }),
    ).toBeNull();
  });
  it("has labelled controls, keyboard access and no automated accessibility violations", async () => {
    const f = fixture();
    const { container } = render(
      <main>
        <KuWorkflowPage client={f.client} />
      </main>,
    );
    await screen.findByRole("option", { name: "Host source" });
    const user = userEvent.setup();
    await user.tab();
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "Refresh host status" }),
    );
    await user.tab();
    expect(document.activeElement?.textContent).toBe("Encode trực tiếp sang KU · luồng thử nghiệm cũ");
    await user.tab();
    expect(document.activeElement?.textContent).toBe("What happens to my sentence?");
    await user.tab();
    expect(document.activeElement).toBe(
      screen.getByLabelText("Admitted source"),
    );
    const result = await axe.run(container, {
      rules: { "color-contrast": { enabled: false } },
    });
    expect(result.violations).toEqual([]);
  });
  it("requires explicit preview/save, projects exact IDs and revisions with keyboard-labelled controls", async () => {
    const f = fixture();
    render(<KuWorkflowPage client={f.client} />);
    await fill();
    expect(f.calls.some((c) => c.body?.request?.operation === "save")).toBe(
      false,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Look up Registry concepts" }),
    );
    await screen.findByRole("option", { name: ccid });
    fireEvent.change(screen.getByLabelText("Explicit concept selection"), {
      target: { value: ccid },
    });
    await preview();
    expect(
      f.calls.filter((c) => c.body?.request?.operation === "save"),
    ).toHaveLength(0);
    fireEvent.click(
      screen.getByRole("button", { name: "Save exact preview privately" }),
    );
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("committed"),
    );
    const save = f.calls.find((c) => c.body?.request?.operation === "save")!
      .body.request.payload;
    expect(save).toEqual({
      operation_id: op,
      idempotency_key: op,
      object_cids: [object],
    });
    fireEvent.click(screen.getByRole("button", { name: "Search / list" }));
    fireEvent.click(
      await screen.findByRole("button", { name: `Inspect ${object}` }),
    );
    await screen.findByRole("heading", { name: "Inspect saved artifact" });
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Create revision",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole("button", { name: "Create revision" }));
    expect(
      screen.getByRole("heading", { name: "Revise as a new private artifact" }),
    ).toBe(document.activeElement);
    fireEvent.change(
      screen.getByLabelText("Text argument (manual assertion)"),
      { target: { value: "Revised assertion" } },
    );
    await preview();
    const revision = f.calls.find(
      (c) => c.body?.request?.operation === "revise",
    )!.body.request.payload;
    expect(revision.predecessor_object_cid).toBe(object);
    expect(revision.expected_revision_frontier).toBe("a".repeat(64));
    expect(
      f.calls.every((c) =>
        c.path.startsWith("http://local.test/api/vnext/ku/"),
      ),
    ).toBe(true);
    expect(f.calls.every((c) => c.init.cache === "no-store")).toBe(true);
  });
  it("keeps unresolved validation visible and disables save", async () => {
    const f = fixture({ unresolved: true });
    render(<KuWorkflowPage client={f.client} />);
    await fill();
    await preview();
    expect(screen.getAllByText("needs_resolution").length).toBeGreaterThan(0);
    expect(
      (
        screen.getByRole("button", {
          name: "Save exact preview privately",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(f.calls.some((c) => c.body?.request?.operation === "save")).toBe(
      false,
    );
  });
  it("requires reconciliation after the host generation changes", async () => {
    const f = fixture();
    render(<KuWorkflowPage client={f.client} />);
    await fill();
    await preview();
    f.rotate();
    fireEvent.click(
      screen.getByRole("button", { name: "Refresh host status" }),
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Reconcile operation",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    expect(
      (
        screen.getByRole("button", {
          name: "Save exact preview privately",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.click(
      screen.getByRole("button", { name: "Reconcile operation" }),
    );
    await waitFor(() =>
      expect(
        (
          screen.getByRole("button", {
            name: "Save exact preview privately",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(false),
    );
    expect(
      f.calls.filter((c) => c.body?.request?.operation === "prepare"),
    ).toHaveLength(1);
    expect(
      f.calls.filter((c) => c.body?.request?.operation === "save"),
    ).toHaveLength(0);
  });
  it("reconciles a lost save response without replay or losing the operation ID", async () => {
    const f = fixture({ loseSave: true });
    render(<KuWorkflowPage client={f.client} />);
    await fill();
    await preview();
    fireEvent.click(
      screen.getByRole("button", { name: "Save exact preview privately" }),
    );
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain(
        "connection lost",
      ),
    );
    expect(
      (
        screen.getByRole("button", {
          name: "Save exact preview privately",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.click(
      screen.getByRole("button", { name: "Reconcile operation" }),
    );
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain("committed"),
    );
    expect(
      f.calls.filter((c) => c.body?.request?.operation === "save"),
    ).toHaveLength(1);
    expect(
      f.calls.find((c) => c.body?.request?.operation === "reconcile")!.body
        .request.payload.operation_id,
    ).toBe(op);
  });
  it("shows and reads saved local work when the editor dependency is unavailable", async () => {
    const f = fixture({ noEditor: true });
    render(<KuWorkflowPage client={f.client} />);
    await screen.findByText(/Manual editor unavailable/);
    fireEvent.click(
      await screen.findByRole("button", { name: `Inspect ${object}` }),
    );
    await screen.findByRole("heading", { name: "Inspect saved artifact" });
    expect(f.calls.some((c) => c.body?.request?.operation === "prepare")).toBe(
      false,
    );
  });
  it("allows a new reservation after its reply is lost, since preparation was never sent", async () => {
    const f = fixture({ reserveLoss: true });
    render(<KuWorkflowPage client={f.client} />);
    await fill();
    fireEvent.click(
      screen.getByRole("button", { name: "Preview and validate" }),
    );
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain(
        "connection lost",
      ),
    );
    expect(f.calls.some((c) => c.body?.request?.action === "draft")).toBe(
      false,
    );
    await preview();
  });
  it("projects D-045 Public KU share preview, separate confirmation, disabled-network preservation, intent status and receiver inspection without exposing private CIDs", async () => {
    const options = { emptyCatalog: true, shareNetworkDisabled: true };
    const f = fixture(options);
    render(<KuWorkflowPage client={f.client} />);
    fireEvent.click(
      await screen.findByRole("button", { name: `Inspect ${object}` }),
    );
    await screen.findByRole("heading", { name: "Inspect saved artifact" });
    const peer = "12".repeat(32),
      selector = "ab".repeat(32),
      namespace = "cd".repeat(32),
      publicObject = "d".repeat(64),
      preparedShareId = "e".repeat(64),
      shareIntent = "f".repeat(64);
    fireEvent.change(
      screen.getByLabelText("Recipient NodeID (64 lowercase hex)"),
      { target: { value: peer } },
    );
    fireEvent.change(screen.getByLabelText("Peer loopback address"), {
      target: { value: "127.0.0.1:41999" },
    });
    fireEvent.change(screen.getByLabelText("Selector CID (64 lowercase hex)"), {
      target: { value: selector },
    });
    fireEvent.change(
      screen.getByLabelText("Namespace commitment (64 lowercase hex)"),
      { target: { value: namespace } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Prepare public share preview" }),
    );
    const preparedRegion = await screen.findByRole("article", {
      name: "Prepared public share preview",
    });
    expect(preparedRegion.textContent).toContain(publicObject);
    expect(preparedRegion.textContent).toContain(semantic);
    expect(preparedRegion.textContent).toContain("Water is a liquid.");
    expect(preparedRegion.textContent).toContain("Enqueued: false");
    expect(preparedRegion.textContent).toContain("Payload sent: false");
    expect(preparedRegion.textContent).not.toContain(object);
    expect(preparedRegion.textContent).not.toContain(source);
    expect(
      f.calls.find((c) => c.body?.request?.action === "share_prepare")!.body
        .request.payload,
    ).toEqual({
      object_cid: object,
      expected_node_id: peer,
      address: "127.0.0.1:41999",
      selector,
      namespace,
    });
    expect(
      f.calls.some((c) => c.body?.request?.action === "share_confirm"),
    ).toBe(false);
    const confirmBtn = screen.getByRole("button", {
      name: "Confirm public share",
    }) as HTMLButtonElement;
    expect(confirmBtn.disabled).toBe(true);
    fireEvent.click(
      screen.getByLabelText(/Confirm public disclosure of this exact preview/),
    );
    expect(confirmBtn.disabled).toBe(false);
    fireEvent.click(confirmBtn);
    await screen.findByRole("heading", {
      name: "Public KU share network runtime is disabled",
    });
    expect(screen.getByRole("alert").textContent).toContain(
      "Any unexpired prepared share preview remains available in memory",
    );
    expect(
      screen.getByRole("article", { name: "Prepared public share preview" })
        .textContent,
    ).toContain(preparedShareId);
    options.shareNetworkDisabled = false;
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm public share" }),
    );
    const confirmedRegion = await screen.findByRole("article", {
      name: "Confirmed public share intent",
    });
    expect(confirmedRegion.textContent).toContain(shareIntent);
    expect(confirmedRegion.textContent).toContain(publicObject);
    expect(confirmedRegion.textContent).toContain(
      "pending; inspect intent for acknowledgement",
    );
    expect(confirmedRegion.textContent).toContain("Reward authorized: false");
    expect(confirmedRegion.textContent).not.toContain(object);
    expect(
      screen.queryByRole("article", { name: "Prepared public share preview" }),
    ).toBeNull();
    expect(
      f.calls.filter((c) => c.body?.request?.action === "share_confirm").at(-1)!
        .body.request.payload,
    ).toEqual({
      prepared_id: preparedShareId,
      confirm_public: true,
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect share intent" }),
    );
    const intentRegion = await screen.findByRole("article", {
      name: "Outbound share intent status",
    });
    expect(intentRegion.textContent).toContain("Acknowledged");
    expect(intentRegion.textContent).toContain(
      "Claims network completion: false",
    );
    fireEvent.change(
      screen.getByLabelText("Accepted Public ObjectCID (64 lowercase hex)"),
      { target: { value: publicObject } },
    );
    fireEvent.change(
      screen.getByLabelText("Accepted selector CID (64 lowercase hex)"),
      { target: { value: selector } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect accepted Public KU" }),
    );
    const inspectRegion = await screen.findByRole("article", {
      name: "Accepted Public KU inspection",
    });
    expect(inspectRegion.textContent).toContain(publicObject);
    expect(inspectRegion.textContent).toContain(semantic);
    expect(inspectRegion.textContent).toContain(peer);
    expect(inspectRegion.textContent).toContain(
      "transport/selector only; original source withheld and unverifiable",
    );
    expect(inspectRegion.textContent).toContain("Reward authorized: false");
    expect(inspectRegion.textContent).not.toContain(object);
    expect(inspectRegion.textContent).not.toContain(source);
  });
  it.each([
    "prepared_share_missing",
    "public_ku_consent_expired",
  ] as const)(
    "explains %s on share_confirm, clears the stale prepared preview and allows re-preparing a fresh preview",
    async (reason) => {
      const options: Parameters<typeof fixture>[0] = {
        emptyCatalog: true,
        shareConfirmFailure: reason,
      };
      const f = fixture(options);
      render(<KuWorkflowPage client={f.client} />);
      fireEvent.click(
        await screen.findByRole("button", { name: `Inspect ${object}` }),
      );
      await screen.findByRole("heading", { name: "Inspect saved artifact" });
      const peer = "12".repeat(32),
        selector = "ab".repeat(32),
        namespace = "cd".repeat(32),
        preparedShareId = "e".repeat(64),
        freshPreparedShareId = "a".repeat(64);
      fireEvent.change(
        screen.getByLabelText("Recipient NodeID (64 lowercase hex)"),
        { target: { value: peer } },
      );
      fireEvent.change(screen.getByLabelText("Peer loopback address"), {
        target: { value: "127.0.0.1:41999" },
      });
      fireEvent.change(
        screen.getByLabelText("Selector CID (64 lowercase hex)"),
        { target: { value: selector } },
      );
      fireEvent.change(
        screen.getByLabelText("Namespace commitment (64 lowercase hex)"),
        { target: { value: namespace } },
      );
      fireEvent.click(
        screen.getByRole("button", { name: "Prepare public share preview" }),
      );
      const initialPreview = await screen.findByRole("article", {
        name: "Prepared public share preview",
      });
      expect(initialPreview.textContent).toContain(preparedShareId);
      fireEvent.click(
        screen.getByLabelText(/Confirm public disclosure of this exact preview/),
      );
      fireEvent.click(
        screen.getByRole("button", { name: "Confirm public share" }),
      );
      await screen.findByRole("heading", {
        name: "Prepared Public KU share preview is no longer valid",
      });
      const alertText = screen.getByRole("alert").textContent ?? "";
      expect(alertText).toContain("15-minute (900-second)");
      expect(alertText).toContain("16-item in-memory preparation limit");
      expect(alertText).toContain("already used once");
      expect(alertText).toContain("host restarted");
      expect(alertText).toContain(
        "Click Prepare public share preview again to create and review a fresh preview before confirming public disclosure.",
      );
      expect(alertText).toContain(reason);
      expect(
        screen.queryByRole("article", { name: "Prepared public share preview" }),
      ).toBeNull();
      expect(
        screen.queryByRole("article", { name: "Confirmed public share intent" }),
      ).toBeNull();
      options.shareConfirmFailure = undefined;
      fireEvent.click(
        screen.getByRole("button", { name: "Prepare public share preview" }),
      );
      const freshPreview = await screen.findByRole("article", {
        name: "Prepared public share preview",
      });
      expect(freshPreview.textContent).toContain(freshPreparedShareId);
      const confirmBtn = screen.getByRole("button", {
        name: "Confirm public share",
      }) as HTMLButtonElement;
      expect(confirmBtn.disabled).toBe(true);
      fireEvent.click(
        screen.getByLabelText(/Confirm public disclosure of this exact preview/),
      );
      fireEvent.click(confirmBtn);
      await screen.findByRole("article", {
        name: "Confirmed public share intent",
      });
      expect(
        f.calls.filter((c) => c.body?.request?.action === "share_confirm").at(-1)!
          .body.request.payload,
      ).toEqual({
        prepared_id: freshPreparedShareId,
        confirm_public: true,
      });
    },
  );
  it.each([
    "unsupported_public_ku",
    "prepared_limit",
    "saved_ku_not_accessible",
  ] as const)(
    "explains %s on share_prepare without creating a preview or implying an AI timeout",
    async (reason) => {
      const options: Parameters<typeof fixture>[0] = {
        emptyCatalog: true,
        sharePrepareFailure: reason,
      };
      const f = fixture(options);
      render(<KuWorkflowPage client={f.client} />);
      fireEvent.click(
        await screen.findByRole("button", { name: `Inspect ${object}` }),
      );
      await screen.findByRole("heading", { name: "Inspect saved artifact" });
      const peer = "12".repeat(32),
        selector = "ab".repeat(32),
        namespace = "cd".repeat(32),
        preparedShareId = "e".repeat(64);
      fireEvent.change(
        screen.getByLabelText("Recipient NodeID (64 lowercase hex)"),
        { target: { value: peer } },
      );
      fireEvent.change(screen.getByLabelText("Peer loopback address"), {
        target: { value: "127.0.0.1:41999" },
      });
      fireEvent.change(
        screen.getByLabelText("Selector CID (64 lowercase hex)"),
        { target: { value: selector } },
      );
      fireEvent.change(
        screen.getByLabelText("Namespace commitment (64 lowercase hex)"),
        { target: { value: namespace } },
      );
      fireEvent.click(
        screen.getByRole("button", { name: "Prepare public share preview" }),
      );
      await screen.findByRole("heading", {
        name:
          reason === "unsupported_public_ku"
            ? "KU is not eligible for D-045 Public share"
            : reason === "saved_ku_not_accessible"
              ? "Saved private KU is no longer accessible for share preparation"
              : "In-memory Public KU share preparation limit reached",
      });
      const alertText = screen.getByRole("alert").textContent ?? "";
      expect(alertText).toContain(reason);
      expect(alertText).not.toContain("600-second (10-minute)");
      expect(alertText).not.toContain(
        "Use Reconcile operation to read the host's recorded outcome",
      );
      if (reason === "unsupported_public_ku") {
        expect(alertText).toContain(
          "one resolved Registry predicate and one text literal",
        );
        expect(alertText).toContain(
          "fails closed rather than stripping private or meaning-bearing content",
        );
        expect(alertText).toContain(
          "No public preview, public object or outbound transfer intent was created",
        );
      } else if (reason === "saved_ku_not_accessible") {
        expect(alertText).toContain(
          "requested private ObjectCID could not be read from the current local Vault dataset",
        );
        expect(alertText).toContain(
          "No public preview, public object or outbound transfer intent was created",
        );
        expect(alertText).toContain(
          "Use Search / list and Inspect to load an accepted LOCAL_ONLY artifact under the current host session before clicking Prepare public share preview.",
        );
      } else {
        expect(alertText).toContain(
          "16 unexpired single-use Public KU share previews in memory",
        );
        expect(alertText).toContain("not an AI model timeout or quota");
        expect(alertText).toContain(
          "Confirm an existing prepared share preview if intended, wait for older 15-minute preparations to expire, or restart the host",
        );
      }
      expect(
        screen.queryByRole("article", { name: "Prepared public share preview" }),
      ).toBeNull();
      options.sharePrepareFailure = undefined;
      fireEvent.click(
        screen.getByRole("button", { name: "Prepare public share preview" }),
      );
      const previewCard = await screen.findByRole("article", {
        name: "Prepared public share preview",
      });
      expect(previewCard.textContent).toContain(preparedShareId);
    },
  );
  it("explains saved_access_changed on share_confirm, intent_not_found on share_intent and public_object_not_found on share_inspect", async () => {
    const options: Parameters<typeof fixture>[0] = {
      emptyCatalog: true,
      shareConfirmFailure: "saved_access_changed",
      shareIntentFailure: "intent_not_found",
      shareInspectFailure: "public_object_not_found",
    };
    const f = fixture(options);
    render(<KuWorkflowPage client={f.client} />);
    fireEvent.click(
      await screen.findByRole("button", { name: `Inspect ${object}` }),
    );
    await screen.findByRole("heading", { name: "Inspect saved artifact" });
    const peer = "12".repeat(32),
      selector = "ab".repeat(32),
      namespace = "cd".repeat(32),
      publicObject = "d".repeat(64);
    fireEvent.change(
      screen.getByLabelText("Recipient NodeID (64 lowercase hex)"),
      { target: { value: peer } },
    );
    fireEvent.change(screen.getByLabelText("Peer loopback address"), {
      target: { value: "127.0.0.1:41999" },
    });
    fireEvent.change(
      screen.getByLabelText("Selector CID (64 lowercase hex)"),
      { target: { value: selector } },
    );
    fireEvent.change(
      screen.getByLabelText("Namespace commitment (64 lowercase hex)"),
      { target: { value: namespace } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Prepare public share preview" }),
    );
    await screen.findByRole("article", {
      name: "Prepared public share preview",
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect share intent" }),
    );
    await screen.findByRole("heading", {
      name: "Outbound Public KU share intent was not found",
    });
    const intentAlert = screen.getByRole("alert").textContent ?? "";
    expect(intentAlert).toContain(
      "Preparing a preview computes the deterministic Intent ID in memory (enqueued: false), but the durable intent is created only after Confirm public share succeeds",
    );
    expect(intentAlert).toContain(
      "If you have only prepared a preview, complete Confirm public share first",
    );
    expect(intentAlert).toContain("intent_not_found");
    expect(
      screen.queryByRole("article", { name: "Outbound share intent status" }),
    ).toBeNull();
    fireEvent.click(
      screen.getByLabelText(/Confirm public disclosure of this exact preview/),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm public share" }),
    );
    await screen.findByRole("heading", {
      name: "Saved private KU access changed before share confirmation",
    });
    const confirmAlert = screen.getByRole("alert").textContent ?? "";
    expect(confirmAlert).toContain(
      "no longer accessible to the current principal or dataset, or no longer matches the exact LOCAL_ONLY accepted_known ObjectCID and canonical bytes",
    );
    expect(confirmAlert).toContain(
      "No payload was enqueued or sent, and the invalidated share preview has been cleared from this page.",
    );
    expect(confirmAlert).toContain(
      "Use Search / list and Inspect to verify the saved LOCAL_ONLY artifact under the current host session, then click Prepare public share preview again before confirming.",
    );
    expect(confirmAlert).toContain("saved_access_changed");
    expect(
      screen.queryByRole("article", { name: "Prepared public share preview" }),
    ).toBeNull();
    expect(
      screen.queryByRole("article", { name: "Confirmed public share intent" }),
    ).toBeNull();
    fireEvent.change(
      screen.getByLabelText("Accepted Public ObjectCID (64 lowercase hex)"),
      { target: { value: publicObject } },
    );
    fireEvent.change(
      screen.getByLabelText("Accepted selector CID (64 lowercase hex)"),
      { target: { value: selector } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect accepted Public KU" }),
    );
    await screen.findByRole("heading", {
      name: "Accepted Public KU was not found in the local public store",
    });
    const inspectAlert = screen.getByRole("alert").textContent ?? "";
    expect(inspectAlert).toContain(
      "canonical verified public store (vnext_verified.redb) does not contain an accepted Public KU",
    );
    expect(inspectAlert).toContain(
      "Verify that you entered the new Public ObjectCID (not the private ObjectCID), check Outbound share intent status on the sender until it reaches Acknowledged",
    );
    expect(inspectAlert).toContain("public_object_not_found");
    expect(
      screen.queryByRole("article", { name: "Accepted Public KU inspection" }),
    ).toBeNull();
  });
  it("explains D-045 outbox and public store DependencyUnavailable errors without Base reconcile misattribution and clears consumed preview on outbox_unavailable_reprepare_same_intent", async () => {
    const options: Parameters<typeof fixture>[0] = {
      emptyCatalog: true,
      shareConfirmFailure: "outbox_unavailable_reprepare_same_intent",
      shareIntentFailure: "outbox_unavailable",
      shareInspectFailure: "public_store_unavailable",
    };
    const f = fixture(options);
    render(<KuWorkflowPage client={f.client} />);
    fireEvent.click(
      await screen.findByRole("button", { name: `Inspect ${object}` }),
    );
    await screen.findByRole("heading", { name: "Inspect saved artifact" });
    const peer = "12".repeat(32),
      selector = "ab".repeat(32),
      namespace = "cd".repeat(32),
      publicObject = "d".repeat(64),
      preparedShareId = "e".repeat(64),
      freshPreparedShareId = "a".repeat(64),
      shareIntent = "f".repeat(64);
    fireEvent.change(
      screen.getByLabelText("Recipient NodeID (64 lowercase hex)"),
      { target: { value: peer } },
    );
    fireEvent.change(screen.getByLabelText("Peer loopback address"), {
      target: { value: "127.0.0.1:41999" },
    });
    fireEvent.change(
      screen.getByLabelText("Selector CID (64 lowercase hex)"),
      { target: { value: selector } },
    );
    fireEvent.change(
      screen.getByLabelText("Namespace commitment (64 lowercase hex)"),
      { target: { value: namespace } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Prepare public share preview" }),
    );
    const initialPreview = await screen.findByRole("article", {
      name: "Prepared public share preview",
    });
    expect(initialPreview.textContent).toContain(preparedShareId);
    fireEvent.click(
      screen.getByLabelText(/Confirm public disclosure of this exact preview/),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm public share" }),
    );
    await screen.findByRole("heading", {
      name: "Durable outbound share outbox was unavailable during confirmation",
    });
    const confirmAlert = screen.getByRole("alert").textContent ?? "";
    expect(confirmAlert).toContain(
      "verified and consumed the single-use share preparation, but its active VNextNetworkRuntime could not confirm durable outbox persistence",
    );
    expect(confirmAlert).toContain(
      "The single-use preview has been cleared from this page; re-preparing the same saved artifact with the same recipient, selector and namespace reproduces the exact deterministic Intent ID and Public ObjectCID.",
    );
    expect(confirmAlert).toContain(
      "Click Inspect share intent first to check whether the deterministic Intent ID was recorded, or click Prepare public share preview again with the same recipient, selector and namespace",
    );
    expect(confirmAlert).toContain("outbox_unavailable_reprepare_same_intent");
    expect(confirmAlert).not.toContain(
      "Use Reconcile operation to read the host's recorded outcome",
    );
    expect(
      screen.queryByRole("article", { name: "Prepared public share preview" }),
    ).toBeNull();
    expect(
      (
        screen.getByLabelText(
          "Outbound intent ID (64 lowercase hex)",
        ) as HTMLInputElement
      ).value,
    ).toBe(shareIntent);
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect share intent" }),
    );
    await screen.findByRole("heading", {
      name: "Durable outbound share outbox is currently unavailable",
    });
    const intentAlert = screen.getByRole("alert").textContent ?? "";
    expect(intentAlert).toContain(
      "could not read the local durable outbound intent store while checking this Intent ID",
    );
    expect(intentAlert).toContain(
      "Verify that the host's network outbox storage is healthy and accessible",
    );
    expect(intentAlert).toContain("outbox_unavailable");
    expect(intentAlert).not.toContain(
      "Use Reconcile operation to read the host's recorded outcome",
    );
    fireEvent.change(
      screen.getByLabelText("Accepted Public ObjectCID (64 lowercase hex)"),
      { target: { value: publicObject } },
    );
    fireEvent.change(
      screen.getByLabelText("Accepted selector CID (64 lowercase hex)"),
      { target: { value: selector } },
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect accepted Public KU" }),
    );
    await screen.findByRole("heading", {
      name: "Local canonical public store is currently unavailable",
    });
    const storeAlert = screen.getByRole("alert").textContent ?? "";
    expect(storeAlert).toContain(
      "could not read the canonical verified public store (vnext_verified.redb)",
    );
    expect(storeAlert).toContain("public_store_unavailable");
    expect(storeAlert).not.toContain(
      "Use Reconcile operation to read the host's recorded outcome",
    );
    options.shareInspectFailure = "provenance_unavailable";
    fireEvent.click(
      screen.getByRole("button", { name: "Inspect accepted Public KU" }),
    );
    await screen.findByRole("heading", {
      name: "Accepted Public KU transport provenance is currently unavailable",
    });
    const provenanceAlert = screen.getByRole("alert").textContent ?? "";
    expect(provenanceAlert).toContain(
      "decoded the accepted Public KU from vnext_verified.redb, but its active VNextNetworkRuntime could not read the authenticated transport/selector provenance index",
    );
    expect(provenanceAlert).toContain("provenance_unavailable");
    expect(provenanceAlert).not.toContain(
      "Use Reconcile operation to read the host's recorded outcome",
    );
    options.shareConfirmFailure = undefined;
    fireEvent.click(
      screen.getByRole("button", { name: "Prepare public share preview" }),
    );
    const freshPreview = await screen.findByRole("article", {
      name: "Prepared public share preview",
    });
    expect(freshPreview.textContent).toContain(freshPreparedShareId);
    fireEvent.click(
      screen.getByLabelText(/Confirm public disclosure of this exact preview/),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm public share" }),
    );
    await screen.findByRole("article", {
      name: "Confirmed public share intent",
    });
    expect(
      f.calls.filter((c) => c.body?.request?.action === "share_confirm").at(-1)!
        .body.request.payload,
    ).toEqual({
      prepared_id: freshPreparedShareId,
      confirm_public: true,
    });
  });
});
