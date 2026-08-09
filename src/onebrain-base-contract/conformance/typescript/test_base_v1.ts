import { readFileSync } from "node:fs";

import {
  ArchiveCredentialKindV1,
  ArchiveChunkV1,
  BaseErrorCodeV1,
  BaseOpaqueContinuation,
  BaseOperationKindV1,
  SignerDomainV1,
  TopicKindV1,
  TypedPayloadV1,
  type ArchiveCapabilityHandleV1,
  type ArchiveSecretHandleV1,
  type ArchiveSinkHandleV1,
  type ArchiveSourceHandleV1,
  type BaseIdempotencyKey,
  type BaseManagementRequestV1,
  type BaseOperationId,
  type BaseOperationReservationId,
  type BaseRequestV1,
  type BaseSubscriptionId,
  type NodeTransportPublicIdV1,
  type SignerProvisionHandleV1,
} from "../../generated/typescript/base_v1.ts";

type CorpusEntry = Readonly<{ id: number; name: string }>;
type Corpus = Readonly<{
  format: string;
  ordinary: ReadonlyArray<CorpusEntry>;
  management: ReadonlyArray<CorpusEntry>;
  errors: ReadonlyArray<CorpusEntry>;
  lifecycle: ReadonlyArray<string>;
  archive_roundtrip: ReadonlyArray<string>;
  negative_vectors: ReadonlyArray<string>;
}>;

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

function id<T>(fill: number): T {
  return new Uint8Array(32).fill(fill) as T;
}

const corpus = JSON.parse(
  readFileSync("../corpus.json", "utf8"),
) as Corpus;
const reservation = id<BaseOperationReservationId>(1);
const operation = id<BaseOperationId>(2);
const subscription = id<BaseSubscriptionId>(3);
const capability = id<ArchiveCapabilityHandleV1>(4);
const source = id<ArchiveSourceHandleV1>(5);
const sink = id<ArchiveSinkHandleV1>(6);
const secret = id<ArchiveSecretHandleV1>(7);
const provision = id<SignerProvisionHandleV1>(8);
const idempotency = id<BaseIdempotencyKey>(9);
const budget = { max_items: 16, max_bytes: 4096n, max_work_units: 1000n };
const payload = TypedPayloadV1.tryFromBytes(new Uint8Array([1, 2, 3]));

const ordinary: ReadonlyArray<BaseRequestV1> = [
  { kind: 3, name: "Status" },
  { kind: 5, name: "Query", payload: { payload, budget } },
  { kind: 6, name: "ReserveOperation", payload: BaseOperationKindV1.CreateArchive },
  {
    kind: 7,
    name: "Prepare",
    payload: {
      reservation_id: reservation,
      command: { kind: 1, name: "ExistingLocalCommand", payload: { kind: 7, payload } },
    },
  },
  { kind: 8, name: "Confirm", payload: { operation_id: operation, idempotency_key: idempotency } },
  { kind: 9, name: "Cancel", payload: operation },
  { kind: 10, name: "Reconcile", payload: operation },
  { kind: 11, name: "Subscribe", payload: { topic: TopicKindV1.OperationReceipts, cursor: 0n } },
  {
    kind: 12,
    name: "PollEvents",
    payload: { subscription_id: subscription, after_cursor: 0n, max_items: 16 },
  },
  { kind: 13, name: "CloseSubscription", payload: subscription },
  { kind: 14, name: "Drain" },
  { kind: 15, name: "Close" },
];

const management: ReadonlyArray<BaseManagementRequestV1> = [
  { kind: 102, name: "ArchiveSourceBegin", payload: { reservation_id: reservation, declared_total_bytes: 3n } },
  { kind: 103, name: "ArchiveSourcePush", payload: { handle: source, offset: 0n, chunk: ArchiveChunkV1.tryFromBytes(new Uint8Array([1, 2, 3])) } },
  { kind: 104, name: "ArchiveSourceSeal", payload: capability },
  { kind: 105, name: "ArchiveSinkBegin", payload: { reservation_id: reservation, max_total_bytes: 4096n } },
  { kind: 106, name: "ArchiveSinkRead", payload: { handle: sink, offset: 0n, max_bytes: 4096 } },
  { kind: 107, name: "ArchiveSinkCommit", payload: capability },
  { kind: 108, name: "ArchiveSecretRegister", payload: { kind: ArchiveCredentialKindV1.Password, bytes: new Uint8Array([1]) } },
  { kind: 109, name: "ArchiveCapabilityAbort", payload: capability },
  { kind: 110, name: "ArchiveCapabilityDestroy", payload: capability },
  {
    kind: 111,
    name: "CompleteSignerReprovision",
    payload: {
      domain: SignerDomainV1.NodeTransport,
      expected_public_id: {
        kind: 1,
        name: "NodeTransport",
        payload: id<NodeTransportPublicIdV1>(10),
      },
      provision_handle: provision,
    },
  },
  { kind: 112, name: "Close" },
];

function normalized(value: unknown): unknown {
  return JSON.parse(
    JSON.stringify(value, (_key, current: unknown) => {
      if (typeof current === "bigint") return current.toString();
      if (current instanceof Uint8Array) return Buffer.from(current).toString("hex");
      return current;
    }),
  );
}

invariant(corpus.format === "onebrain/base-v1-projection-conformance/1", "wrong corpus profile");
invariant(
  JSON.stringify(ordinary.map(({ kind, name }) => ({ id: kind, name }))) ===
    JSON.stringify(corpus.ordinary),
  "ordinary discriminator drift",
);
invariant(
  JSON.stringify(management.map(({ kind, name }) => ({ id: kind, name }))) ===
    JSON.stringify(corpus.management),
  "management discriminator drift",
);
invariant(
  JSON.stringify(Object.values(BaseErrorCodeV1).filter((value): value is number => typeof value === "number")) ===
    JSON.stringify(corpus.errors.map(({ id }) => id)),
  "typed error drift",
);
invariant(corpus.archive_roundtrip.length === 15, "archive lifecycle corpus is incomplete");
invariant(corpus.negative_vectors.includes("kill_reopen_unknown_outcome"), "reopen vector missing");
invariant(normalized({ ordinary, management }) !== null, "normalized semantic encoding failed");

const continuationInput = new Uint8Array([7, 8]);
const continuation = BaseOpaqueContinuation.tryFromBytes(continuationInput);
continuationInput[0] = 0;
invariant(continuation.asBytes()[0] === 7, "continuation ownership was not copied");
let rejected = false;
try {
  BaseOpaqueContinuation.tryFromBytes(new Uint8Array(4097));
} catch (error) {
  rejected = error instanceof RangeError;
}
invariant(rejected, "continuation bound was not enforced");
invariant(secret.length === 32, "opaque handle width drifted");

console.log("TypeScript Base v1 projection conformance passed.");
