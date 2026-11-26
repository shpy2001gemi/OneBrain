/**
 * Pillar 10 §4.1 (`AI-003`) — Content-Script Sanitized WebCapture.
 *
 * Strips sensitive HTTP headers, URL credentials & query/fragment tokens,
 * `<input type="password">`, sensitive `autocomplete` controls, hidden
 * `<input type="hidden">` CSRF/auth values, `<script>`, `<iframe>`, and inline
 * `on*` / `javascript:` handlers BEFORE bytes leave the content script.
 */

export const SENSITIVE_HEADER_NAMES: ReadonlySet<string> = new Set([
  "cookie",
  "set-cookie",
  "authorization",
  "proxy-authorization",
  "x-api-key",
  "x-onebrain-obp-management",
]);

export const SENSITIVE_URL_PARAM_NAMES: ReadonlySet<string> = new Set([
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
]);

const SENSITIVE_AUTOCOMPLETE_EXACT: ReadonlySet<string> = new Set([
  "current-password",
  "new-password",
  "one-time-code",
]);

const PAIRED_STRIP_TAGS: ReadonlySet<string> = new Set([
  "script",
  "iframe",
]);

export interface RawWebCaptureInput {
  readonly url: string;
  readonly title: string;
  readonly html: string;
  readonly selectionText?: string;
  readonly headers?: Readonly<Record<string, string>> | Iterable<readonly [string, string]>;
  readonly capturedAtMs?: number;
}

export interface SanitizedHeadersResult {
  readonly headers: Record<string, string>;
  readonly stripped_header_names: string[];
}

export interface SanitizedUrlResult {
  readonly sanitized_url: string;
  readonly stripped_param_names: string[];
  readonly stripped_userinfo: boolean;
}

export interface SanitizedHtmlResult {
  readonly sanitized_html: string;
  readonly redacted_element_count: number;
}

export interface SanitizedWebCaptureV1 {
  readonly schema_version: "onebrain.web_capture.v1";
  readonly artifact_kind: "SourceArtifact(WebCapture)";
  readonly sanitized_url: string;
  readonly title: string;
  readonly sanitized_html: string;
  readonly selection_text: string;
  readonly sanitized_headers: Record<string, string>;
  readonly stripped_header_names: string[];
  readonly stripped_param_names: string[];
  readonly stripped_userinfo: boolean;
  readonly redacted_element_count: number;
  readonly captured_at_ms: number;
  readonly epistemic_badge: "[Private · Local Only]";
}

export class WebCaptureSanitizerError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "WebCaptureSanitizerError";
    this.code = code;
  }
}

export function sanitizeCaptureHeaders(
  input?: Readonly<Record<string, string>> | Iterable<readonly [string, string]>,
): SanitizedHeadersResult {
  const sanitized: Record<string, string> = {};
  const stripped: string[] = [];
  if (!input) {
    return { headers: sanitized, stripped_header_names: stripped };
  }

  const entries: Iterable<readonly [string, string]> =
    Symbol.iterator in Object(input)
      ? (input as Iterable<readonly [string, string]>)
      : Object.entries(input as Readonly<Record<string, string>>);

  for (const [rawName, rawValue] of entries) {
    const normalized = rawName.trim().toLowerCase();
    if (!normalized) {
      continue;
    }
    if (SENSITIVE_HEADER_NAMES.has(normalized)) {
      stripped.push(normalized);
      continue;
    }
    sanitized[rawName.trim()] = String(rawValue);
  }

  return {
    headers: sanitized,
    stripped_header_names: stripped,
  };
}

function isSensitiveParamName(name: string): boolean {
  return SENSITIVE_URL_PARAM_NAMES.has(name.trim().toLowerCase());
}

function sanitizeSearchParamsString(
  rawQuery: string,
  strippedOut: string[],
): string {
  const params = new URLSearchParams(rawQuery);
  const kept = new URLSearchParams();
  for (const [key, value] of params.entries()) {
    if (isSensitiveParamName(key)) {
      strippedOut.push(key.toLowerCase());
    } else {
      kept.append(key, value);
    }
  }
  return kept.toString();
}

export function sanitizeCaptureUrl(rawUrl: string): SanitizedUrlResult {
  let parsed: URL;
  try {
    parsed = new URL(rawUrl);
  } catch {
    throw new WebCaptureSanitizerError(
      "INVALID_CAPTURE_URL",
      "Capture URL could not be parsed as an absolute URL",
    );
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new WebCaptureSanitizerError(
      "UNSUPPORTED_CAPTURE_URL_SCHEME",
      `WebCapture permits only http: and https: URLs (got ${parsed.protocol})`,
    );
  }

  const strippedParams: string[] = [];
  let strippedUserinfo = false;

  if (parsed.username !== "" || parsed.password !== "") {
    parsed.username = "";
    parsed.password = "";
    strippedUserinfo = true;
  }

  if (parsed.search.length > 1) {
    const cleanedSearch = sanitizeSearchParamsString(
      parsed.search.slice(1),
      strippedParams,
    );
    parsed.search = cleanedSearch ? `?${cleanedSearch}` : "";
  }

  if (parsed.hash.length > 1) {
    const rawHashBody = parsed.hash.slice(1);
    const hashQueryIdx = rawHashBody.indexOf("?");
    if (hashQueryIdx >= 0) {
      const routePart = rawHashBody.slice(0, hashQueryIdx);
      const queryPart = rawHashBody.slice(hashQueryIdx + 1);
      const cleanedHashQuery = sanitizeSearchParamsString(queryPart, strippedParams);
      parsed.hash = cleanedHashQuery
        ? `#${routePart}?${cleanedHashQuery}`
        : routePart
          ? `#${routePart}`
          : "";
    } else if (rawHashBody.includes("=")) {
      const cleanedFragmentParams = sanitizeSearchParamsString(
        rawHashBody,
        strippedParams,
      );
      parsed.hash = cleanedFragmentParams ? `#${cleanedFragmentParams}` : "";
    }
  }

  return {
    sanitized_url: parsed.toString(),
    stripped_param_names: strippedParams,
    stripped_userinfo: strippedUserinfo,
  };
}

interface ParsedAttribute {
  readonly rawName: string;
  readonly lowerName: string;
  readonly value: string;
  readonly rawText: string;
}

function parseTagAttributes(attrSource: string): ParsedAttribute[] {
  const attrs: ParsedAttribute[] = [];
  let i = 0;
  const n = attrSource.length;

  while (i < n) {
    while (i < n && /\s/.test(attrSource[i]!)) {
      i++;
    }
    if (i >= n || attrSource[i] === "/") {
      if (attrSource[i] === "/") {
        i++;
      }
      continue;
    }

    const start = i;
    while (i < n && !/[\s=/>]/.test(attrSource[i]!)) {
      i++;
    }
    const rawName = attrSource.slice(start, i);
    if (!rawName) {
      i++;
      continue;
    }

    let wsAfterName = i;
    while (wsAfterName < n && /\s/.test(attrSource[wsAfterName]!)) {
      wsAfterName++;
    }

    let value = "";
    if (wsAfterName < n && attrSource[wsAfterName] === "=") {
      i = wsAfterName + 1;
      while (i < n && /\s/.test(attrSource[i]!)) {
        i++;
      }
      if (i < n && (attrSource[i] === '"' || attrSource[i] === "'")) {
        const quote = attrSource[i]!;
        i++;
        const valStart = i;
        while (i < n && attrSource[i] !== quote) {
          i++;
        }
        value = attrSource.slice(valStart, i);
        if (i < n && attrSource[i] === quote) {
          i++;
        }
      } else {
        const valStart = i;
        while (i < n && !/[\s>]/.test(attrSource[i]!)) {
          i++;
        }
        value = attrSource.slice(valStart, i);
      }
    }

    attrs.push({
      rawName,
      lowerName: rawName.toLowerCase(),
      value,
      rawText: attrSource.slice(start, i),
    });
  }

  return attrs;
}

function hasSensitiveAutocomplete(value: string): boolean {
  const tokens = value
    .trim()
    .toLowerCase()
    .split(/\s+/)
    .filter((token) => token.length > 0);
  for (const token of tokens) {
    if (SENSITIVE_AUTOCOMPLETE_EXACT.has(token) || token.startsWith("cc-")) {
      return true;
    }
  }
  return false;
}

function isDangerousUriValue(value: string): boolean {
  const normalized = value.replace(/[\u0000-\u0020]+/g, "").toLowerCase();
  return (
    normalized.startsWith("javascript:") ||
    normalized.startsWith("vbscript:") ||
    normalized.startsWith("data:text/html")
  );
}

function shouldStripElement(
  lowerTag: string,
  attrs: readonly ParsedAttribute[],
): boolean {
  if (PAIRED_STRIP_TAGS.has(lowerTag)) {
    return true;
  }

  for (const attr of attrs) {
    if (attr.lowerName === "autocomplete" && hasSensitiveAutocomplete(attr.value)) {
      return true;
    }
  }

  if (lowerTag === "input") {
    for (const attr of attrs) {
      if (attr.lowerName === "type") {
        const inputType = attr.value.trim().toLowerCase();
        if (inputType === "password" || inputType === "hidden") {
          return true;
        }
      }
    }
  }

  return false;
}

function findTagEnd(html: string, openBracketIdx: number): number {
  let quote: string | null = null;
  for (let i = openBracketIdx + 1; i < html.length; i++) {
    const ch = html[i]!;
    if (quote !== null) {
      if (ch === quote) {
        quote = null;
      }
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      continue;
    }
    if (ch === ">") {
      return i;
    }
  }
  return -1;
}

function findMatchingClosingTagEnd(
  html: string,
  searchFrom: number,
  lowerTag: string,
): number {
  const closePattern = new RegExp(`<\\s*\\/\\s*${lowerTag}\\b[^>]*>`, "gi");
  closePattern.lastIndex = searchFrom;
  const match = closePattern.exec(html);
  if (!match) {
    return html.length;
  }
  return match.index + match[0].length;
}

const VOID_ELEMENTS: ReadonlySet<string> = new Set([
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr",
]);

export function sanitizeCaptureHtml(rawHtml: string): SanitizedHtmlResult {
  let redactedElementCount = 0;
  const out: string[] = [];
  let cursor = 0;

  while (cursor < rawHtml.length) {
    const lt = rawHtml.indexOf("<", cursor);
    if (lt < 0) {
      out.push(rawHtml.slice(cursor));
      break;
    }

    if (lt > cursor) {
      out.push(rawHtml.slice(cursor, lt));
    }

    if (rawHtml.startsWith("<!--", lt)) {
      const commentEnd = rawHtml.indexOf("-->", lt + 4);
      if (commentEnd < 0) {
        break;
      }
      cursor = commentEnd + 3;
      continue;
    }

    const gt = findTagEnd(rawHtml, lt);
    if (gt < 0) {
      break;
    }

    const inside = rawHtml.slice(lt + 1, gt);
    const trimmedInside = inside.trim();

    if (trimmedInside.startsWith("/")) {
      const closeTagMatch = /^\/\s*([a-zA-Z0-9:-]+)/.exec(trimmedInside);
      const closeName = closeTagMatch?.[1]?.toLowerCase() ?? "";
      if (PAIRED_STRIP_TAGS.has(closeName)) {
        cursor = gt + 1;
        continue;
      }
      out.push(rawHtml.slice(lt, gt + 1));
      cursor = gt + 1;
      continue;
    }

    const openMatch = /^([a-zA-Z0-9:-]+)([\s\S]*)$/.exec(trimmedInside);
    if (!openMatch) {
      out.push(rawHtml.slice(lt, gt + 1));
      cursor = gt + 1;
      continue;
    }

    const rawTagName = openMatch[1]!;
    const lowerTag = rawTagName.toLowerCase();
    const rawAttrPart = openMatch[2] ?? "";
    const selfClosing = /\/\s*$/.test(rawAttrPart);
    const attrs = parseTagAttributes(rawAttrPart);

    if (shouldStripElement(lowerTag, attrs)) {
      redactedElementCount += 1;
      if (!selfClosing && !VOID_ELEMENTS.has(lowerTag)) {
        cursor = findMatchingClosingTagEnd(rawHtml, gt + 1, lowerTag);
      } else {
        cursor = gt + 1;
      }
      continue;
    }

    let strippedAttrOnElement = false;
    const keptAttrs: string[] = [];
    for (const attr of attrs) {
      if (attr.lowerName.startsWith("on")) {
        strippedAttrOnElement = true;
        continue;
      }
      if (
        (attr.lowerName === "href" ||
          attr.lowerName === "src" ||
          attr.lowerName === "action" ||
          attr.lowerName === "formaction") &&
        isDangerousUriValue(attr.value)
      ) {
        strippedAttrOnElement = true;
        continue;
      }
      keptAttrs.push(attr.rawText);
    }

    if (strippedAttrOnElement) {
      redactedElementCount += 1;
    }

    const rebuiltAttrs = keptAttrs.length > 0 ? ` ${keptAttrs.join(" ")}` : "";
    const rebuiltClose = selfClosing ? " />" : ">";
    out.push(`<${rawTagName}${rebuiltAttrs}${rebuiltClose}`);
    cursor = gt + 1;
  }

  return {
    sanitized_html: out.join(""),
    redacted_element_count: redactedElementCount,
  };
}

export function sanitizeWebCapture(input: RawWebCaptureInput): SanitizedWebCaptureV1 {
  const urlResult = sanitizeCaptureUrl(input.url);
  const headerResult = sanitizeCaptureHeaders(input.headers);
  const htmlResult = sanitizeCaptureHtml(input.html);

  return {
    schema_version: "onebrain.web_capture.v1",
    artifact_kind: "SourceArtifact(WebCapture)",
    sanitized_url: urlResult.sanitized_url,
    title: input.title.trim(),
    sanitized_html: htmlResult.sanitized_html,
    selection_text: (input.selectionText ?? "").trim(),
    sanitized_headers: headerResult.headers,
    stripped_header_names: headerResult.stripped_header_names,
    stripped_param_names: urlResult.stripped_param_names,
    stripped_userinfo: urlResult.stripped_userinfo,
    redacted_element_count: htmlResult.redacted_element_count,
    captured_at_ms: input.capturedAtMs ?? Date.now(),
    epistemic_badge: "[Private · Local Only]",
  };
}
