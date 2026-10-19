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
export interface Models {
  models: {
    model: string;
    implementation_commitment: Preparation["implementation_commitment"];
    experimental: true;
  }[];
  limitations: string[];
  consent_text: string;
}
export interface TextIntake {
  operation_id: OperationRef["operation_id"];
  idempotency_key: Preparation["idempotency_key"];
  model: string;
  text: string;
  consent: boolean;
}
export interface ReviewClaim {
  alternatives?: { quote: string; cue: string; exclusivity: string; branches: {surface: {quote:string}; borrowed?: {quote:string}; origin:string}[] }[];
  ellipses?: {surface:{quote:string};borrowed:{quote:string};origin:string}[];
  references?: {via:{quote:string};to:{quote:string};target_kind:string;target_id?:string;status?:string}[];
  comparisons?: {property:string;cue:string;target_kind:string;target?:{quote:string};origin:string}[];
  evidence_spans?: {role:string;quote:string;start:number;end:number}[];
  subject: string;
  predicate: string;
  arguments: string[];
  frequency: string[];
  negation: string[];
  condition: string[];
  time: string[];
  location: string[];
  modality: string[];
  approximation: string[];
  numbers: { value_quote: string; unit_quote: string; counted_entity_quote: string }[];
  relations: { statement: number; kind: string; quote: string }[];
  evidence: string;
}
export interface ReviewJob {
  operation_id: string;
  source: string;
  model: string;
  canonical_ku: false;
  factual_verification: string;
  semantic_verification?: string;
  job: {
    state: string;
    calls: number;
    charged_ms: number;
    active?: { window: number } | null;
    issues: string[];
    windows: {
      start: number; end: number; state: string;
      revisions: {
        draft: { profile?: string; statements: ReviewClaim[]; unresolved: { quote: string; reason: string }[] };
        validation: { issues: string[]; uncovered: string[] };
      }[];
      reviews: { unresolved: { quote: string; reason: string }[]; missing: string[] }[];
    }[];
  };
}
export interface ReviewLowerInput {
  operation_id: OperationRef["operation_id"];
  idempotency_key: Preparation["idempotency_key"];
  review_operation_id: string;
  window: number;
  statement: number;
  reviewed: true;
}
export interface ReviewLowering {
  saveable: boolean;
  preparation?: Preparation;
  unresolved: { role: string; label: string; candidates: number }[];
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
export interface PublicSharePrepareInput {
  object_cid: string;
  expected_node_id: string;
  address: string;
  selector: string;
  namespace: string;
}
export interface PublicShareSemantic {
  semantic_content_cid: string;
  statements: {
    predicate_ccid: string;
    text_arguments: (string | null)[];
  }[];
  fidelity: "unassessed";
}
export interface PublicSharePrepared {
  event: "share_prepared";
  prepared_id: string;
  intent_id: string;
  public_object_cid: string;
  semantic_content_cid: string;
  canonical_base64: string;
  semantic: PublicShareSemantic;
  expected_node_id: string;
  address: string;
  selector: string;
  namespace: string;
  expires_at: number;
  consequence: string;
  enqueued: false;
  payload_sent: false;
  support: string;
}
export interface PublicShareConfirmInput {
  prepared_id: string;
  confirm_public: boolean;
}
export interface PublicShareConfirmed {
  event: "share_confirmed";
  intent_id: string;
  public_object_cid: string;
  outbox: string;
  delivery: string;
  authorizes_reward: false;
}
export interface PublicShareIntentInput {
  intent_id: string;
}
export interface PublicShareIntentView {
  event: "intent";
  intent_id: string;
  public_object_cid: string;
  state: string;
  terminal_sequence: number | null;
  claims_network_completion: false;
}
export interface PublicShareInspectInput {
  object_cid: string;
  selector: string;
}
export interface PublicShareObjectView {
  event: "public_object";
  object_cid: string;
  canonical_base64: string;
  semantic: PublicShareSemantic;
  authenticated_source_peers: string[];
  provenance: string;
  fidelity: "unassessed";
  authorizes_reward: false;
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
  status: [OperationRef, Status];
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
    models: (session: Session) =>
      request<Models>("editor", {
        session,
        budget,
        request: { action: "models", payload: {} },
      }),
    encodeText: (session: Session, payload: TextIntake) =>
      request<Preparation>("editor", {
        session,
        budget,
        request: { action: "encode_text", payload },
      }),
    reviewStart: (session: Session, payload: TextIntake) =>
      request<{review_job: ReviewJob}>("editor", {session, budget, request:{action:"review_start",payload}}),
    reviewGet: (session: Session, operation_id: string) =>
      request<{review_job: ReviewJob}>("editor", {session, budget, request:{action:"review_get",payload:{operation_id}}}),
    reviewList: (session: Session) =>
      request<{review_jobs: {operation_id:string; model:string; state:string; created_ms:number}[]}>("editor", {session, budget, request:{action:"review_list",payload:{}}}),
    reviewResume: (session: Session, operation_id: string) =>
      request<{review_job: ReviewJob}>("editor", {session, budget, request:{action:"review_resume",payload:{operation_id}}}),
    reviewCancel: (session: Session, operation_id: string) =>
      request<{review_job: ReviewJob}>("editor", {session, budget, request:{action:"review_cancel",payload:{operation_id}}}),
    reviewLower: (session: Session, payload: ReviewLowerInput) =>
      request<{review_lowering: ReviewLowering}>("editor", {session, budget, request:{action:"review_lower",payload}}),
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
    sharePrepare: (session: Session, payload: PublicSharePrepareInput) =>
      request<PublicSharePrepared>("editor", {
        session,
        budget,
        request: { action: "share_prepare", payload },
      }),
    shareConfirm: (session: Session, payload: PublicShareConfirmInput) =>
      request<PublicShareConfirmed>("editor", {
        session,
        budget,
        request: { action: "share_confirm", payload },
      }),
    shareIntent: (session: Session, payload: PublicShareIntentInput) =>
      request<PublicShareIntentView>("editor", {
        session,
        budget,
        request: { action: "share_intent", payload },
      }),
    shareInspect: (session: Session, payload: PublicShareInspectInput) =>
      request<PublicShareObjectView>("editor", {
        session,
        budget,
        request: { action: "share_inspect", payload },
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
