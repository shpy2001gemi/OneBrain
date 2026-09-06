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
