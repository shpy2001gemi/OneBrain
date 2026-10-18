import { expect, it, vi } from "vitest";
import { createKuClient, KuError } from "../src/api/ku";
it("preserves typed Base error policy without logging private request or response bodies", async () => {
  const log = vi.spyOn(console, "log"),
    error = vi.spyOn(console, "error");
  const failure = {
    code: "UnknownOutcome",
    retryable: false,
    reconcile_before_retry: true,
    limitations: ["reconcile_before_retry"],
  };
  const fetcher = vi.fn(
    async () =>
      new Response(
        JSON.stringify({ ok: false, error: { code: "conflict", failure } }),
        { status: 409 },
      ),
  );
  const client = createKuClient(
    async () => ({ baseUrl: "http://local", token: "secret" }),
    fetcher,
  );
  try {
    await client.status();
    throw new Error("expected rejection");
  } catch (e) {
    expect(e).toBeInstanceOf(KuError);
    expect((e as KuError).failure).toEqual(failure);
    expect((e as KuError).uncertain).toBe(true);
  }
  expect(log).not.toHaveBeenCalled();
  expect(error).not.toHaveBeenCalled();
  log.mockRestore();
  error.mockRestore();
});
it("projects sharePrepare, shareConfirm, shareIntent and shareInspect through POST /api/vnext/ku/editor without logging", async () => {
  const log = vi.spyOn(console, "log"),
    error = vi.spyOn(console, "error");
  const session = {
    process_generation: "6".repeat(64),
    dataset_generation: "7".repeat(64),
  };
  const calls: { url: string; init: RequestInit }[] = [];
  const fetcher = vi.fn(async (url: string | URL | Request, init?: RequestInit) => {
    calls.push({ url: String(url), init: init! });
    return new Response(
      JSON.stringify({
        ok: true,
        data: { session, payload: { event: "ok" }, model_qualified: false },
        meta: {
          lifecycle: "active",
          coverage: "local_only",
          limitations: [],
          continuation: null,
        },
      }),
      { status: 200 },
    );
  });
  const client = createKuClient(
    async () => ({ baseUrl: "http://local", token: "secret" }),
    fetcher as typeof fetch,
  );
  await client.sharePrepare(session, {
    object_cid: "1".repeat(64),
    expected_node_id: "2".repeat(64),
    address: "127.0.0.1:41999",
    selector: "3".repeat(64),
    namespace: "4".repeat(64),
  });
  await client.shareConfirm(session, {
    prepared_id: "5".repeat(64),
    confirm_public: true,
  });
  await client.shareIntent(session, { intent_id: "8".repeat(64) });
  await client.shareInspect(session, {
    object_cid: "9".repeat(64),
    selector: "3".repeat(64),
  });
  expect(
    calls.map((c) => JSON.parse(String(c.init.body)).request.action),
  ).toEqual(["share_prepare", "share_confirm", "share_intent", "share_inspect"]);
  expect(
    calls.every(
      (c) =>
        c.url === "http://local/api/vnext/ku/editor" &&
        c.init.cache === "no-store" &&
        c.init.credentials === "omit",
    ),
  ).toBe(true);
  expect(log).not.toHaveBeenCalled();
  expect(error).not.toHaveBeenCalled();
  log.mockRestore();
  error.mockRestore();
});
it("treats 507 storage_full as non-retryable, not uncertain, with a storage explanation (ARCH-03)", async () => {
  const { describeKuError } = await import("../src/pages/kuFeedback");
  const failure = {
    code: "ResourceExhausted",
    retryable: false,
    reconcile_before_retry: false,
    limitations: ["ku_storage_full"],
  };
  const client = createKuClient(
    async () => ({ baseUrl: "http://local", token: "secret" }),
    vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            ok: false,
            error: { code: "storage_full", retryable: false, failure },
          }),
          { status: 507 },
        ),
    ),
  );
  let caught: unknown;
  try {
    await client.status();
  } catch (e) {
    caught = e;
  }
  expect(caught).toBeInstanceOf(KuError);
  expect((caught as KuError).uncertain).toBe(false);
  const feedback = describeKuError(caught);
  expect(feedback.title).toBe("Local KU storage limit reached");
  expect(feedback.context).toContain("not retryable");
  expect(feedback.recovery).toContain("Save or cancel pending drafts");
  expect(feedback.recovery).not.toContain("Reconcile operation");
  expect(feedback.isShareError).toBe(false);
});
