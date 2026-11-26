/**
 * Pillar 10 §4.1 (`AI-003`) — Isolated-World Content Script.
 *
 * Performs URL, header, and DOM/HTML sanitization BEFORE transmitting bytes
 * across the WebExtension message channel to the background service worker.
 */

import {
  sanitizeWebCapture,
  type RawWebCaptureInput,
  type SanitizedWebCaptureV1,
} from "./sanitizer.ts";

export interface ContentScriptCaptureMessage {
  readonly type: "OB_CONTENT_CAPTURE_PAGE";
  readonly headers?: Readonly<Record<string, string>>;
}

export interface ContentScriptCaptureResponse {
  readonly ok: boolean;
  readonly capture?: SanitizedWebCaptureV1;
  readonly error_code?: string;
  readonly error_message?: string;
}

export function captureFromPageSnapshot(
  snapshot: RawWebCaptureInput,
): SanitizedWebCaptureV1 {
  return sanitizeWebCapture(snapshot);
}

export function readCurrentDocumentSnapshot(
  extraHeaders?: Readonly<Record<string, string>>,
): RawWebCaptureInput {
  const loc = globalThis.location?.href ?? "https://example.invalid/";
  const docTitle = globalThis.document?.title ?? "";
  const bodyHtml = globalThis.document?.body?.innerHTML ?? "";
  const selectionText = globalThis.getSelection?.()?.toString() ?? "";

  return {
    url: loc,
    title: docTitle,
    html: bodyHtml,
    selectionText,
    headers: extraHeaders,
    capturedAtMs: Date.now(),
  };
}

export function handleContentScriptMessage(
  message: unknown,
  snapshotProvider: () => RawWebCaptureInput = readCurrentDocumentSnapshot,
): ContentScriptCaptureResponse {
  if (
    typeof message !== "object" ||
    message === null ||
    (message as { type?: unknown }).type !== "OB_CONTENT_CAPTURE_PAGE"
  ) {
    return {
      ok: false,
      error_code: "UNSUPPORTED_CONTENT_MESSAGE",
      error_message: "Unsupported content script message type",
    };
  }

  try {
    const rawSnapshot = snapshotProvider();
    const msgHeaders = (message as ContentScriptCaptureMessage).headers;
    const capture = captureFromPageSnapshot(
      msgHeaders ? { ...rawSnapshot, headers: msgHeaders } : rawSnapshot,
    );
    return { ok: true, capture };
  } catch (err) {
    const code =
      typeof err === "object" && err !== null && "code" in err
        ? String((err as { code: unknown }).code)
        : "CAPTURE_SANITIZATION_FAILED";
    const errorMessage = err instanceof Error ? err.message : String(err);
    return {
      ok: false,
      error_code: code,
      error_message: errorMessage,
    };
  }
}

interface ChromeRuntimeLike {
  onMessage?: {
    addListener(
      listener: (
        message: unknown,
        sender: unknown,
        sendResponse: (resp: ContentScriptCaptureResponse) => void,
      ) => boolean | void,
    ): void;
  };
}

const maybeChrome = (globalThis as { chrome?: { runtime?: ChromeRuntimeLike } }).chrome;
if (maybeChrome?.runtime?.onMessage) {
  maybeChrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    sendResponse(handleContentScriptMessage(message));
    return false;
  });
}
