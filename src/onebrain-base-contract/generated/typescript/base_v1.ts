// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.
export const BASE_RUNTIME_PROFILE_MAJOR = 1 as const;
export const BASE_RUNTIME_PROFILE_MINOR = 0 as const;

export type CapabilitySetV1 = ReadonlyArray<number> & { readonly __maxItems: 64 };

export type CompatibilityDigestV1 = Uint8Array & { readonly __brand: "CompatibilityDigestV1" };

export type DatasetGenerationV1 = bigint & { readonly __brand: "DatasetGenerationV1" };

export type EventCursorV1 = bigint & { readonly __brand: "EventCursorV1" };

export type IdempotencyKeyV1 = Uint8Array & { readonly __brand: "IdempotencyKeyV1" };

export type LimitationCodeV1 = string & { readonly __maxBytes: 128 };

export type ManagementHandleV1 = Uint8Array & { readonly __brand: "ManagementHandleV1" };

export class OpaqueContinuationV1 {
  private constructor(private readonly value: Uint8Array) {}

  static tryFromBytes(bytes: Uint8Array): OpaqueContinuationV1 {
    if (bytes.length > 4096) throw new RangeError("OpaqueContinuationV1 exceeds 4096 bytes");
    return new OpaqueContinuationV1(bytes.slice());
  }

  asBytes(): Uint8Array {
    return this.value.slice();
  }
}

export type OperationIdV1 = Uint8Array & { readonly __brand: "OperationIdV1" };

export type OperationReservationIdV1 = Uint8Array & { readonly __brand: "OperationReservationIdV1" };

export type ProcessGenerationV1 = bigint & { readonly __brand: "ProcessGenerationV1" };

export type ProfileMajorV1 = number & { readonly __brand: "ProfileMajorV1" };

export type ProfileMinorV1 = number & { readonly __brand: "ProfileMinorV1" };

export type RequestIdV1 = Uint8Array & { readonly __brand: "RequestIdV1" };

export type SubscriptionHandleV1 = Uint8Array & { readonly __brand: "SubscriptionHandleV1" };

export class TypedPayloadV1 {
  private constructor(private readonly value: Uint8Array) {}

  static tryFromBytes(bytes: Uint8Array): TypedPayloadV1 {
    if (bytes.length > 1048576) throw new RangeError("TypedPayloadV1 exceeds 1048576 bytes");
    return new TypedPayloadV1(bytes.slice());
  }

  asBytes(): Uint8Array {
    return this.value.slice();
  }
}

export type ActorRootPublicIdV1 = Uint8Array & { readonly __brand: "ActorRootPublicIdV1" };

export type ArchiveCapabilityHandleV1 = Uint8Array & { readonly __brand: "ArchiveCapabilityHandleV1" };

export class ArchiveChunkV1 {
  private constructor(private readonly value: Uint8Array) {}

  static tryFromBytes(bytes: Uint8Array): ArchiveChunkV1 {
    if (bytes.length > 1048576) throw new RangeError("ArchiveChunkV1 exceeds 1048576 bytes");
    return new ArchiveChunkV1(bytes.slice());
  }

  asBytes(): Uint8Array {
    return this.value.slice();
  }
}

export enum ArchiveCredentialKindV1 {
  Password = 1,
  RecoveryKey = 2,
}

export type ArchiveSecretHandleV1 = Uint8Array & { readonly __brand: "ArchiveSecretHandleV1" };

export interface ArchiveSinkBeginV1 {
  readonly reservation_id: BaseOperationReservationId;
  readonly max_total_bytes: bigint;
}

export type ArchiveSinkHandleV1 = Uint8Array & { readonly __brand: "ArchiveSinkHandleV1" };

export interface ArchiveSinkReadV1 {
  readonly handle: ArchiveSinkHandleV1;
  readonly offset: bigint;
  readonly max_bytes: number;
}

export interface ArchiveSourceBeginV1 {
  readonly reservation_id: BaseOperationReservationId;
  readonly declared_total_bytes: bigint;
}

export type ArchiveSourceHandleV1 = Uint8Array & { readonly __brand: "ArchiveSourceHandleV1" };

export interface ArchiveSourcePushV1 {
  readonly handle: ArchiveSourceHandleV1;
  readonly offset: bigint;
  readonly chunk: ArchiveChunkV1;
}

export type BaseCommandV1 =
  | { readonly kind: 1; readonly name: "ExistingLocalCommand"; readonly payload: BaseLocalCommandV1 }
  | { readonly kind: 2; readonly name: "CreateArchive"; readonly payload: CreateArchiveCommandV1 }
  | { readonly kind: 3; readonly name: "RestoreArchive"; readonly payload: RestoreArchiveCommandV1 };

export interface BaseConfirmRequestV1 {
  readonly operation_id: BaseOperationId;
  readonly idempotency_key: BaseIdempotencyKey;
}

export enum BaseErrorCodeV1 {
  InvalidRequest = 1,
  NotFound = 2,
  Conflict = 3,
  Expired = 4,
  RateLimited = 5,
  CapabilityDisabled = 6,
  DependencyUnavailable = 7,
  IncompatibleProfile = 8,
  ResourceExhausted = 9,
  CorruptState = 10,
  ReprovisionRequired = 11,
  UnknownOutcome = 12,
  InternalError = 13,
}

export type BaseIdempotencyKey = Uint8Array & { readonly __brand: "BaseIdempotencyKey" };

export interface BaseLocalCommandV1 {
  readonly kind: number;
  readonly payload: TypedPayloadV1;
}

export type BaseManagementGrantV1 = Uint8Array & { readonly __brand: "BaseManagementGrantV1" };

export type BaseManagementRequestV1 =
  | { readonly kind: 102; readonly name: "ArchiveSourceBegin"; readonly payload: ArchiveSourceBeginV1 }
  | { readonly kind: 103; readonly name: "ArchiveSourcePush"; readonly payload: ArchiveSourcePushV1 }
  | { readonly kind: 104; readonly name: "ArchiveSourceSeal"; readonly payload: ArchiveCapabilityHandleV1 }
  | { readonly kind: 105; readonly name: "ArchiveSinkBegin"; readonly payload: ArchiveSinkBeginV1 }
  | { readonly kind: 106; readonly name: "ArchiveSinkRead"; readonly payload: ArchiveSinkReadV1 }
  | { readonly kind: 107; readonly name: "ArchiveSinkCommit"; readonly payload: ArchiveCapabilityHandleV1 }
  | { readonly kind: 108; readonly name: "ArchiveSecretRegister"; readonly payload: BoundedSecretIngressV1 }
  | { readonly kind: 109; readonly name: "ArchiveCapabilityAbort"; readonly payload: ArchiveCapabilityHandleV1 }
  | { readonly kind: 110; readonly name: "ArchiveCapabilityDestroy"; readonly payload: ArchiveCapabilityHandleV1 }
  | { readonly kind: 111; readonly name: "CompleteSignerReprovision"; readonly payload: CompleteSignerReprovisionV1 }
  | { readonly kind: 112; readonly name: "Close" };

export class BaseOpaqueContinuation {
  private constructor(private readonly value: Uint8Array) {}

  static tryFromBytes(bytes: Uint8Array): BaseOpaqueContinuation {
    if (bytes.length > 4096) throw new RangeError("BaseOpaqueContinuation exceeds 4096 bytes");
    return new BaseOpaqueContinuation(bytes.slice());
  }

  asBytes(): Uint8Array {
    return this.value.slice();
  }
}

export type BaseOperationId = Uint8Array & { readonly __brand: "BaseOperationId" };

export enum BaseOperationKindV1 {
  ExistingLocalCommand = 1,
  CreateArchive = 2,
  RestoreArchive = 3,
}

export type BaseOperationReservationId = Uint8Array & { readonly __brand: "BaseOperationReservationId" };

export interface BasePollEventsRequestV1 {
  readonly subscription_id: BaseSubscriptionId;
  readonly after_cursor: bigint;
  readonly max_items: number;
}

export interface BasePrepareRequestV1 {
  readonly reservation_id: BaseOperationReservationId;
  readonly command: BaseCommandV1;
}

export interface BaseQueryRequestV1 {
  readonly payload: TypedPayloadV1;
  readonly continuation?: BaseOpaqueContinuation;
  readonly budget: ResourceBudgetV1;
}

export type BaseRequestV1 =
  | { readonly kind: 3; readonly name: "Status" }
  | { readonly kind: 5; readonly name: "Query"; readonly payload: BaseQueryRequestV1 }
  | { readonly kind: 6; readonly name: "ReserveOperation"; readonly payload: BaseOperationKindV1 }
  | { readonly kind: 7; readonly name: "Prepare"; readonly payload: BasePrepareRequestV1 }
  | { readonly kind: 8; readonly name: "Confirm"; readonly payload: BaseConfirmRequestV1 }
  | { readonly kind: 9; readonly name: "Cancel"; readonly payload: BaseOperationId }
  | { readonly kind: 10; readonly name: "Reconcile"; readonly payload: BaseOperationId }
  | { readonly kind: 11; readonly name: "Subscribe"; readonly payload: BaseSubscriptionRequestV1 }
  | { readonly kind: 12; readonly name: "PollEvents"; readonly payload: BasePollEventsRequestV1 }
  | { readonly kind: 13; readonly name: "CloseSubscription"; readonly payload: BaseSubscriptionId }
  | { readonly kind: 14; readonly name: "Drain" }
  | { readonly kind: 15; readonly name: "Close" };

export type BaseSubscriptionId = Uint8Array & { readonly __brand: "BaseSubscriptionId" };

export interface BaseSubscriptionRequestV1 {
  readonly topic: TopicKindV1;
  readonly cursor?: bigint;
}

export interface BoundedSecretIngressV1 {
  readonly kind: ArchiveCredentialKindV1;
  readonly bytes: Uint8Array;
}

export interface CompleteSignerReprovisionV1 {
  readonly domain: SignerDomainV1;
  readonly expected_public_id: SignerPublicIdV1;
  readonly provision_handle: SignerProvisionHandleV1;
}

export interface CreateArchiveCommandV1 {
  readonly sink: ArchiveSinkHandleV1;
  readonly secret: ArchiveSecretHandleV1;
  readonly budget: ResourceBudgetV1;
}

export type FeedAuthorPublicIdV1 = Uint8Array & { readonly __brand: "FeedAuthorPublicIdV1" };

export type NodeTransportPublicIdV1 = Uint8Array & { readonly __brand: "NodeTransportPublicIdV1" };

export interface ResourceBudgetV1 {
  readonly max_items: number;
  readonly max_bytes: bigint;
  readonly max_work_units: bigint;
}

export interface RestoreArchiveCommandV1 {
  readonly source: ArchiveSourceHandleV1;
  readonly secret: ArchiveSecretHandleV1;
  readonly budget: ResourceBudgetV1;
}

export enum SignerDomainV1 {
  NodeTransport = 1,
  ActorRoot = 2,
  FeedAuthor = 3,
}

export type SignerProvisionHandleV1 = Uint8Array & { readonly __brand: "SignerProvisionHandleV1" };

export type SignerPublicIdV1 =
  | { readonly kind: 1; readonly name: "NodeTransport"; readonly payload: NodeTransportPublicIdV1 }
  | { readonly kind: 2; readonly name: "ActorRoot"; readonly payload: ActorRootPublicIdV1 }
  | { readonly kind: 3; readonly name: "FeedAuthor"; readonly payload: FeedAuthorPublicIdV1 };

export enum TopicKindV1 {
  RuntimeStatus = 1,
  OperationReceipts = 2,
  QueryResults = 3,
  ArchiveProgress = 4,
  Compatibility = 5,
}
