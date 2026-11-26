import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { describe, it } from "node:test";

import {
  dispatchBackgroundMessage,
} from "../src/background.ts";
import {
  assertCanonicalEpistemicBadge,
  buildHostConfirmationHandoff,
  ExtensionCapabilityClient,
  ExtensionCapabilityError,
  LOOPBACK_HOST_ORIGIN,
  validateExtensionSurfaceToken,
} from "../src/capabilityClient.ts";
import {
  captureFromPageSnapshot,
  handleContentScriptMessage,
} from "../src/contentScript.ts";
import {
  applyCapturedDraftToPopup,
  createInitialPopupViewModel,
} from "../src/popup.ts";
import {
  sanitizeCaptureHeaders,
  sanitizeCaptureHtml,
  sanitizeCaptureUrl,
  sanitizeWebCapture,
  WebCaptureSanitizerError,
} from "../src/sanitizer.ts";

function toBase64Url(input: string | Uint8Array): string {
  const bytes =
    typeof input === "string" ? new TextEncoder().encode(input) : input;
  let binary = "";
  for (const b of bytes) {
    binary += String.fromCharCode(b);
  }
  return globalThis
    .btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}

function makeExtensionSurfaceToken(overrides?: Record<string, unknown>): string {
  const claims = {
    surface_kind: "extension",
    allowed_origin: LOOPBACK_HOST_ORIGIN,
    permissions: ["extension:capture:write", "extension:drafts:read"],
    exp_ms: 4_102_444_800_000,
    ...overrides,
  };
  const claimsSegment = toBase64Url(JSON.stringify(claims));
  const macHex = createHash("sha256")
    .update(`obc_surface1.${claimsSegment}`)
    .digest("hex");
  return `obc_surface1.${claimsSegment}.${macHex}`;
}

describe("P10 §4.1 AI-003 Sanitized WebCapture (sanitizer.ts)", () => {
  it("strips sensitive headers case-insensitively while preserving benign headers", () => {
    const result = sanitizeCaptureHeaders({
      "Content-Type": "text/html; charset=utf-8",
      "cOoKiE": "sid=super-secret-cookie",
      "SET-COOKIE": "session=abc123; HttpOnly",
      "Authorization": "Bearer secret-jwt",
      "proxy-authorization": "Basic dXNlcjpwYXNz",
      "X-API-KEY": "sk-live-secret",
      "x-onebrain-obp-management": "obm1.root-admin-secret",
      "Accept-Language": "en-US",
    });

    assert.deepEqual(result.headers, {
      "Content-Type": "text/html; charset=utf-8",
      "Accept-Language": "en-US",
    });
    assert.deepEqual(result.stripped_header_names, [
      "cookie",
      "set-cookie",
      "authorization",
      "proxy-authorization",
      "x-api-key",
      "x-onebrain-obp-management",
    ]);
  });

  it("strips sensitive query & fragment parameters and userinfo from capture URLs", () => {
    const rawUrl =
      "https://alice:hunter2@docs.example.com:8443/guide?topic=rust&access_token=tok1&refresh_token=rt1&id_token=idt&token=t2&session=s1&session_id=sid1&sig=sg&signature=sig2&code=oauth_code&api_key=ak&key=k1&secret=sec1&password=pw1&page=2#/view?tab=notes&access_token=hash_tok&code=hash_code";

    const result = sanitizeCaptureUrl(rawUrl);
    assert.equal(result.stripped_userinfo, true);
    assert.equal(
      result.sanitized_url,
      "https://docs.example.com:8443/guide?topic=rust&page=2#/view?tab=notes",
    );
    assert.ok(!result.sanitized_url.includes("hunter2"));
    assert.ok(!result.sanitized_url.includes("alice"));
    assert.deepEqual(result.stripped_param_names, [
      "access_token",
      "refresh_token",
      "id_token",
      "token",
      "session",
      "session_id",
      "sig",
      "signature",
      "code",
      "api_key",
      "key",
      "secret",
      "password",
      "access_token",
      "code",
    ]);
  });

  it("rejects non-HTTP(S) capture URLs fail-closed", () => {
    assert.throws(
      () => sanitizeCaptureUrl("javascript:alert(document.cookie)"),
      (err: unknown) =>
        err instanceof WebCaptureSanitizerError &&
        err.code === "UNSUPPORTED_CAPTURE_URL_SCHEME",
    );
    assert.throws(
      () => sanitizeCaptureUrl("file:///etc/passwd"),
      (err: unknown) =>
        err instanceof WebCaptureSanitizerError &&
        err.code === "UNSUPPORTED_CAPTURE_URL_SCHEME",
    );
  });

  it("strips password inputs, sensitive autocomplete controls, hidden inputs, scripts, iframes, and inline on* handlers", () => {
    const dirtyHtml = [
      `<article id="main" onclick="steal()" onmouseover='track()'>`,
      `<h1>Distributed Consensus Notes</h1>`,
      `<p>Read <a href="https://example.org/paper" onclick="hijack()">the paper</a> or <a href="javascript:evil()">bad link</a>.</p>`,
      `<form action="/login">`,
      `<input type="text" name="username" value="alice" />`,
      `<input type="PASSWORD" name="pwd" value="top-secret-password" />`,
      `<input type="text" autocomplete="section-login current-password" value="autofilled-pw" />`,
      `<input type="text" autocomplete="new-password" value="new-pw" />`,
      `<input type="text" autocomplete="one-time-code" value="492817" />`,
      `<input type="text" autocomplete="billing cc-number" value="4111222233334444" />`,
      `<input type="hidden" name="csrf_token" value="csrf-secret-999" />`,
      `</form>`,
      `<script>window.__EXFIL__ = document.cookie;</script>`,
      `<iframe src="https://ads.example.net/frame?token=xyz"></iframe>`,
      `</article>`,
    ].join("");

    const result = sanitizeCaptureHtml(dirtyHtml);

    assert.ok(result.sanitized_html.includes("<h1>Distributed Consensus Notes</h1>"));
    assert.ok(result.sanitized_html.includes(`name="username"`));
    assert.ok(!result.sanitized_html.includes("top-secret-password"));
    assert.ok(!result.sanitized_html.includes("autofilled-pw"));
    assert.ok(!result.sanitized_html.includes("new-pw"));
    assert.ok(!result.sanitized_html.includes("492817"));
    assert.ok(!result.sanitized_html.includes("4111222233334444"));
    assert.ok(!result.sanitized_html.includes("csrf-secret-999"));
    assert.ok(!result.sanitized_html.includes("<script"));
    assert.ok(!result.sanitized_html.includes("<iframe"));
    assert.ok(!result.sanitized_html.includes("onclick"));
    assert.ok(!result.sanitized_html.includes("onmouseover"));
    assert.ok(!result.sanitized_html.includes("javascript:"));
    // 6 sensitive inputs + 1 script + 1 iframe + 3 elements with stripped inline/js attrs = 11
    assert.equal(result.redacted_element_count, 11);
  });

  it("sanitizes inside contentScript before bytes cross internal extension IPC", () => {
    const ipcResp = handleContentScriptMessage(
      {
        type: "OB_CONTENT_CAPTURE_PAGE",
        headers: {
          Cookie: "auth=secret",
          Accept: "text/html",
        },
      },
      () => ({
        url: "https://user:pw@app.example.com/item?session=s99&id=42",
        title: "  Research Note  ",
        html: `<div>Safe text<input type="password" value="p@ss" /><script>bad()</script></div>`,
        selectionText: "  Safe text  ",
        capturedAtMs: 1_700_000_000_000,
      }),
    );

    assert.equal(ipcResp.ok, true);
    assert.ok(ipcResp.capture);
    assert.equal(ipcResp.capture.schema_version, "onebrain.web_capture.v1");
    assert.equal(ipcResp.capture.sanitized_url, "https://app.example.com/item?id=42");
    assert.deepEqual(ipcResp.capture.stripped_param_names, ["session"]);
    assert.deepEqual(ipcResp.capture.stripped_header_names, ["cookie"]);
    assert.equal(ipcResp.capture.redacted_element_count, 2);
    assert.equal(ipcResp.capture.epistemic_badge, "[Private · Local Only]");
    assert.ok(!JSON.stringify(ipcResp).includes("p@ss"));
    assert.ok(!JSON.stringify(ipcResp).includes("auth=secret"));

    const direct = captureFromPageSnapshot({
      url: "https://example.com/?token=secret",
      title: "Direct",
      html: "<p>Hello</p>",
    });
    assert.equal(direct.sanitized_url, "https://example.com/");
  });
});

describe("P10 §4.1 Capability Ceiling, NEG-P10-002 & T-BCI-04 (capabilityClient.ts)", () => {
  it("validates obc_surface1 extension tokens and rejects obm1. admin tokens", () => {
    const validToken = makeExtensionSurfaceToken();
    const parsed = validateExtensionSurfaceToken(validToken, 1_700_000_000_000);
    assert.equal(parsed.claims.surface_kind, "extension");
    assert.equal(parsed.mac_bytes_len, 32);

    assert.throws(
      () => validateExtensionSurfaceToken("obm1.admin_root_capability.deadbeef"),
      (err: unknown) =>
        err instanceof ExtensionCapabilityError &&
        err.code === "OBM1_ADMIN_TOKEN_PROHIBITED",
    );

    const botToken = makeExtensionSurfaceToken({ surface_kind: "bot" });
    assert.throws(
      () => validateExtensionSurfaceToken(botToken),
      (err: unknown) =>
        err instanceof ExtensionCapabilityError &&
        err.code === "WRONG_SURFACE_KIND",
    );

    const escalatedToken = makeExtensionSurfaceToken({
      permissions: ["extension:capture:write", "vault:export"],
    });
    assert.throws(
      () => validateExtensionSurfaceToken(escalatedToken),
      (err: unknown) =>
        err instanceof ExtensionCapabilityError &&
        err.code === "SURFACE_CEILING_EXCEEDED",
    );
  });

  it("blocks bulk PrivateVault, Base v1, and P9 somatic endpoints with zero network calls", async () => {
    let fetchCalls = 0;
    const client = new ExtensionCapabilityClient({
      token: makeExtensionSurfaceToken(),
      nowFn: () => 1_700_000_000_000,
      fetchImpl: async () => {
        fetchCalls += 1;
        return { ok: true, status: 200, json: async () => ({}) };
      },
    });

    const blockedVaultPaths = [
      "/api/kus",
      "/api/export",
      "/api/backup",
      "/api/restore",
      "/api/base/v1/vault",
    ];
    for (const path of blockedVaultPaths) {
      await assert.rejects(
        () => client.rawScopedRequest("GET", path),
        (err: unknown) =>
          err instanceof ExtensionCapabilityError &&
          err.code === "BULK_VAULT_ENDPOINT_PROHIBITED",
      );
    }

    const blockedSomaticPaths = [
      "/api/p9/stream",
      "/api/vnext/p9/intent",
      "/api/bci/decode",
      "/api/vnext/somatic/lease",
    ];
    for (const path of blockedSomaticPaths) {
      await assert.rejects(
        () => client.rawScopedRequest("POST", path, {}),
        (err: unknown) =>
          err instanceof ExtensionCapabilityError &&
          err.code === "P9_SOMATIC_ACCESS_PROHIBITED",
      );
    }

    const bgSomatic = await dispatchBackgroundMessage(
      { type: "EFFECT_NEURAL_READ_INTENT" },
      { getSurfaceToken: async () => makeExtensionSurfaceToken() },
    );
    assert.equal(bgSomatic.ok, false);
    if (!bgSomatic.ok) {
      assert.equal(bgSomatic.error_code, "P9_SOMATIC_ACCESS_PROHIBITED");
    }

    assert.equal(fetchCalls, 0);
  });

  it("hard-blocks step-2 PUBLIC confirmation (NEG-P10-002) and returns deterministic HostConfirmationHandoff", async () => {
    let fetchCalls = 0;
    const client = new ExtensionCapabilityClient({
      token: makeExtensionSurfaceToken(),
      nowFn: () => 1_700_000_000_000,
      fetchImpl: async () => {
        fetchCalls += 1;
        return { ok: true, status: 200, json: async () => ({}) };
      },
    });

    const handoff = client.confirmPublicPublication("draft_2026_abc");
    assert.equal(handoff.blocked, true);
    assert.equal(handoff.invariant_id, "NEG-P10-002");
    assert.equal(
      handoff.handoff_url,
      "http://127.0.0.1:4280/ku?review_draft=draft_2026_abc",
    );
    assert.equal(handoff.epistemic_badge, "[Public · Unassessed]");

    await assert.rejects(
      () =>
        client.rawScopedRequest(
          "POST",
          "/api/vnext/extension/drafts/draft_2026_abc/confirm-public",
        ),
      (err: unknown) =>
        err instanceof ExtensionCapabilityError &&
        err.code === "PUBLIC_CONFIRM_PROHIBITED" &&
        err.handoff?.handoff_url ===
          "http://127.0.0.1:4280/ku?review_draft=draft_2026_abc",
    );

    const bgConfirm = await dispatchBackgroundMessage(
      { type: "OB_REQUEST_PUBLIC_CONFIRM", draft_id: "draft_2026_abc" },
      { getSurfaceToken: async () => makeExtensionSurfaceToken() },
    );
    assert.equal(bgConfirm.ok, false);
    if (!bgConfirm.ok) {
      assert.equal(bgConfirm.error_code, "PUBLIC_CONFIRM_PROHIBITED");
      assert.equal(
        bgConfirm.handoff?.handoff_url,
        "http://127.0.0.1:4280/ku?review_draft=draft_2026_abc",
      );
    }

    assert.equal(fetchCalls, 0);
  });

  it("executes scoped capture POST and draft GET with canonical badges and rejects FULL/GLOBAL/CLOSED", async () => {
    const requests: Array<{ url: string; method: string; body?: string }> = [];
    const client = new ExtensionCapabilityClient({
      token: makeExtensionSurfaceToken(),
      nowFn: () => 1_700_000_000_000,
      fetchImpl: async (url, init) => {
        requests.push({
          url,
          method: init?.method ?? "GET",
          body: init?.body,
        });
        if (url.endsWith("/api/vnext/extension/capture")) {
          return {
            ok: true,
            status: 201,
            json: async () => ({
              draft_id: "drf_001",
              artifact_cid: "bafkreiexamplecid",
              epistemic_badge: "[Private · Local Only]",
              reachability_badge: "[Local Store Only]",
              model_qualified: false,
            }),
          };
        }
        return {
          ok: true,
          status: 200,
          json: async () => ({
            draft_id: "drf_001",
            title: "Clean Note",
            sanitized_url: "https://example.org/post",
            summary: "Scoped local preview",
            epistemic_badge: "[Public · Unassessed]",
            reachability_badge: "[Sampled Reachable Peers: 3]",
            model_qualified: false,
          }),
        };
      },
    });

    const sanitized = sanitizeWebCapture({
      url: "https://example.org/post?api_key=leak123",
      title: "Clean Note",
      html: `<p>Body<input type="password" value="secret" /></p>`,
    });

    const receipt = await client.submitCapture(sanitized);
    assert.equal(receipt.draft_id, "drf_001");
    assert.equal(receipt.epistemic_badge, "[Private · Local Only]");
    assert.equal(receipt.reachability_badge, "[Local Store Only]");

    const preview = await client.getDraft("drf_001");
    assert.equal(preview.draft_id, "drf_001");
    assert.equal(preview.epistemic_badge, "[Public · Unassessed]");
    assert.equal(preview.reachability_badge, "[Sampled Reachable Peers: 3]");

    const initialVm = createInitialPopupViewModel();
    assert.equal(initialVm.epistemicBadge, "[Private · Local Only]");
    const updatedVm = applyCapturedDraftToPopup(sanitized, receipt);
    assert.equal(
      updatedVm.hostConfirmationHandoff?.handoff_url,
      "http://127.0.0.1:4280/ku?review_draft=drf_001",
    );
    assert.equal(
      buildHostConfirmationHandoff("drf_001").handoff_url,
      "http://127.0.0.1:4280/ku?review_draft=drf_001",
    );

    assert.equal(requests.length, 2);
    assert.ok(!requests[0]!.body!.includes("leak123"));
    assert.ok(!requests[0]!.body!.includes("secret"));

    for (const legacyLabel of ["[FULL]", "[GLOBAL]", "[CLOSED]"]) {
      assert.throws(
        () => assertCanonicalEpistemicBadge(legacyLabel),
        (err: unknown) =>
          err instanceof ExtensionCapabilityError &&
          err.code === "LEGACY_BADGE_PROHIBITED",
      );
    }
  });
});
