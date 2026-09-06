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
    reserveLoss?: boolean;
    ai?: boolean;
    prepareWait?: Promise<void>;
  } = {},
) {
  let saved = false;
  let currentOp = op;
  let currentSession = { ...session };
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
    state: saved ? "committed" : "prepared",
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
              sources: [{ source_ref: source, label: "Host source" }],
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
        }
      } else {
        switch (body.request.operation) {
          case "prepare":
            if (options.prepareWait) await options.prepareWait;
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
              items: saved || options.noEditor ? [summary] : [],
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
});
