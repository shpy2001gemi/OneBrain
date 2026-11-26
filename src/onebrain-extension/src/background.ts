/**
 * Pillar 10 §4.1 — MV3 Service Worker Background Coordinator.
 *
 * Enforces:
 * - Session-scoped `obc_surface1` token custody (`storage.session` / RAM only)
 * - Scoped `POST /api/vnext/extension/capture` & `GET /api/vnext/extension/drafts/:id`
 * - Hard-block on `P9` somatic streams (`T-BCI-04`)
 * - Hard-block on step-2 `PUBLIC` confirmation (`NEG-P10-002`), returning
 *   deterministic `HostConfirmationHandoff` to the Desktop/Web host UI.
 */

import {
  buildHostConfirmationHandoff,
  ExtensionCapabilityClient,
  ExtensionCapabilityError,
  type CapturedDraftReceiptV1,
  type FetchLike,
  type HostConfirmationHandoff,
  type ScopedDraftPreviewV1,
} from "./capabilityClient.ts";
import type { SanitizedWebCaptureV1 } from "./sanitizer.ts";

export type BackgroundRequestMessage =
  | {
      readonly type: "OB_SUBMIT_SANITIZED_CAPTURE";
      readonly capture: SanitizedWebCaptureV1;
    }
  | {
      readonly type: "OB_GET_DRAFT_PREVIEW";
      readonly draft_id: string;
    }
  | {
      readonly type: "OB_REQUEST_PUBLIC_CONFIRM";
      readonly draft_id: string;
    }
  | {
      readonly type: string;
      readonly [key: string]: unknown;
    };

export type BackgroundResponseMessage =
  | {
      readonly ok: true;
      readonly receipt?: CapturedDraftReceiptV1;
      readonly preview?: ScopedDraftPreviewV1;
      readonly handoff?: HostConfirmationHandoff;
    }
  | {
      readonly ok: false;
      readonly error_code: string;
      readonly error_message: string;
      readonly handoff?: HostConfirmationHandoff;
    };

export interface BackgroundRouterDeps {
  readonly getSurfaceToken: () => Promise<string>;
  readonly fetchImpl?: FetchLike;
  readonly nowFn?: () => number;
}

export async function dispatchBackgroundMessage(
  message: BackgroundRequestMessage,
  deps: BackgroundRouterDeps,
): Promise<BackgroundResponseMessage> {
  const msgType = String(message.type ?? "");

  if (
    msgType.startsWith("OB_P9_") ||
    msgType.startsWith("EFFECT_NEURAL_") ||
    msgType.startsWith("EFFECT_SENSORY_") ||
    msgType.startsWith("EFFECT_ARBITRARY_NEURAL_")
  ) {
    return {
      ok: false,
      error_code: "P9_SOMATIC_ACCESS_PROHIBITED",
      error_message:
        "T-BCI-04: P9 somatic and neural intent streams are strictly prohibited in onebrain-extension",
    };
  }

  if (msgType === "OB_REQUEST_PUBLIC_CONFIRM") {
    try {
      const draftId = String((message as { draft_id?: unknown }).draft_id ?? "");
      const handoff = buildHostConfirmationHandoff(draftId);
      return {
        ok: false,
        error_code: "PUBLIC_CONFIRM_PROHIBITED",
        error_message:
          "NEG-P10-002: Step-2 PUBLIC confirmation is prohibited in onebrain-extension; complete confirmation in OneBrain Desktop/Web host",
        handoff,
      };
    } catch (err) {
      const capabilityErr = err as ExtensionCapabilityError;
      return {
        ok: false,
        error_code: capabilityErr.code ?? "INVALID_DRAFT_ID",
        error_message: capabilityErr.message,
      };
    }
  }

  try {
    const token = await deps.getSurfaceToken();
    const client = new ExtensionCapabilityClient({
      token,
      fetchImpl: deps.fetchImpl,
      nowFn: deps.nowFn,
    });

    if (msgType === "OB_SUBMIT_SANITIZED_CAPTURE") {
      const capture = (message as { capture: SanitizedWebCaptureV1 }).capture;
      const receipt = await client.submitCapture(capture);
      return { ok: true, receipt };
    }

    if (msgType === "OB_GET_DRAFT_PREVIEW") {
      const draftId = String((message as { draft_id?: unknown }).draft_id ?? "");
      const preview = await client.getDraft(draftId);
      return { ok: true, preview };
    }

    return {
      ok: false,
      error_code: "UNSUPPORTED_BACKGROUND_MESSAGE",
      error_message: `Unsupported extension message type: ${msgType}`,
    };
  } catch (err) {
    if (err instanceof ExtensionCapabilityError) {
      return {
        ok: false,
        error_code: err.code,
        error_message: err.message,
        handoff: err.handoff,
      };
    }
    return {
      ok: false,
      error_code: "INTERNAL_EXTENSION_ERROR",
      error_message: err instanceof Error ? err.message : String(err),
    };
  }
}

interface ChromeStorageSessionLike {
  get(key: string): Promise<Record<string, unknown>>;
}

interface ChromeBackgroundGlobal {
  storage?: {
    session?: ChromeStorageSessionLike;
  };
  runtime?: {
    onMessage?: {
      addListener(
        cb: (
          message: BackgroundRequestMessage,
          sender: unknown,
          sendResponse: (resp: BackgroundResponseMessage) => void,
        ) => boolean,
      ): void;
    };
  };
}

const maybeChrome = (globalThis as { chrome?: ChromeBackgroundGlobal }).chrome;
if (maybeChrome?.runtime?.onMessage) {
  maybeChrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    void dispatchBackgroundMessage(message, {
      getSurfaceToken: async () => {
        const stored = await maybeChrome.storage?.session?.get("obc_surface1_token");
        return typeof stored?.obc_surface1_token === "string"
          ? stored.obc_surface1_token
          : "";
      },
    }).then(sendResponse);
    return true;
  });
}
