import type {
  KuPayloadKuPrepareV1 as Preparation,
  KuPayloadKuPreparedV1 as Prepared,
  KuPayloadKuReceiptV1 as Receipt,
  KuPayloadKuStatusV1 as Status,
  KuPayloadKuPageV1 as Page,
  KuPayloadKuViewV1 as View,
  KuPayloadKuFailureV1 as Failure,
  KuPayloadKuOperationRefV1 as OperationRef,
  KuPayloadKuSaveV1 as Save,
  KuPayloadKuReviseV1 as Revise,
  KuPayloadKuListV1 as List,
  KuPayloadKuSearchV1 as Search,
  KuPayloadKuGetV1 as Get,
} from "../../../onebrain-base-contract/generated/typescript/base_v1";
export type {
  Preparation,
  Prepared,
  Receipt,
  Status,
  Page,
  View,
  OperationRef,
};
export interface Session {
  process_generation: string;
  dataset_generation: string;
}
export interface Meta {
  lifecycle: string;
  coverage: string;
  limitations: string[];
  continuation: string | null;
}
export interface Result<T> {
  data: { session: Session; payload: T; model_qualified: false };
  meta: Meta;
}
export interface Catalog {
  sources: { source_ref: Preparation["source_refs"][number]; label: string }[];
  limitations: string[];
}
export interface Candidates {
  candidates: { ccid: string }[];
  limitations: string[];
}
export interface Draft {
  operation_id: OperationRef["operation_id"];
  idempotency_key: Preparation["idempotency_key"];
  source_ref: Preparation["source_refs"][number];
  predicate_label: string;
  selected_ccid?: string;
  argument_text: string;
}
export class KuError extends Error {
  readonly failure?: Failure;
  readonly uncertain: boolean;
  constructor(message: string, failure?: Failure, uncertain = false) {
    super(message);
    this.name = "KuError";
    this.failure = failure;
    this.uncertain = uncertain || !!failure?.reconcile_before_retry;
  }
}
const budget = { max_items: 64, max_bytes: 1048576, max_work_units: 1000000 };
interface Operations {
  prepare: [Preparation, Prepared];
  preview: [OperationRef, Prepared];
  save: [Save, Receipt];
  revise: [Revise, Prepared];
  get: [Get, View];
  list: [List, Page];
  search: [Search, Page];
  cancel: [OperationRef, Receipt];
  reconcile: [OperationRef, Receipt];
}
// Dedicated private transport: never use the legacy debug-logging request path.
export function createKuClient(
  connection: () => Promise<{ baseUrl: string; token: string }>,
  fetcher: typeof fetch = fetch,
) {
  async function request<T>(
    path: string,
    payload?: unknown,
  ): Promise<Result<T>> {
    const { baseUrl, token } = await connection();
    let response: Response;
    try {
      response = await fetcher(`${baseUrl}/api/vnext/ku/${path}`, {
        method: payload === undefined ? "GET" : "POST",
        cache: "no-store",
        credentials: "omit",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${token}`,
        },
        ...(payload === undefined ? {} : { body: JSON.stringify(payload) }),
      });
    } catch {
      throw new KuError(
        "Local API connection lost. Reconcile pending work before retrying.",
        undefined,
        true,
      );
    }
    let envelope;
    try {
      envelope = await response.json();
    } catch {
      throw new KuError(
        "Unreadable API reply. Reconcile pending work.",
        undefined,
        true,
      );
    }
    if (!response.ok || !envelope.ok) {
      throw new KuError(
        envelope.error?.code || "Local API unavailable",
        envelope.error?.failure,
        !envelope.error?.failure,
      );
    }
    if (
      !envelope.data?.session ||
      envelope.data.model_qualified !== false ||
      !envelope.meta
    ) {
      throw new KuError(
        "Unsupported KU response. Reconcile pending work.",
        undefined,
        true,
      );
    }
    return envelope as Result<T>;
  }
  return {
    status: () => request<Status>("status"),
    reserve: (session: Session) =>
      request<OperationRef>("reservations", { session }),
    catalog: (session: Session) =>
      request<Catalog>("editor", {
        session,
        budget,
        request: { action: "catalog", payload: {} },
      }),
    resolve: (session: Session, label: string) =>
      request<Candidates>("editor", {
        session,
        budget,
        request: { action: "resolve", payload: { label } },
      }),
    draft: (session: Session, payload: Draft) =>
      request<Preparation>("editor", {
        session,
        budget,
        request: { action: "draft", payload },
      }),
    invoke: <K extends keyof Operations>(
      session: Session,
      operation: K,
      payload: Operations[K][0],
    ) =>
      request<Operations[K][1]>("operations", {
        session,
        budget,
        request: { operation, payload },
      }),
  };
}
export type KuClient = ReturnType<typeof createKuClient>;
export function canSave(
  prepared?: Prepared,
  receipt?: Receipt,
  uncertain = false,
): boolean {
  return (
    !!prepared &&
    prepared.validity === "ready" &&
    prepared.object_cids.length > 0 &&
    prepared.artifacts.length === prepared.object_cids.length &&
    prepared.artifacts.every(
      (a, i) => a.object_cid === prepared.object_cids[i],
    ) &&
    !uncertain &&
    (!receipt || receipt.state === "prepared")
  );
}
