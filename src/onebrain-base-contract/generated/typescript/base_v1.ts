// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.
export const BASE_RUNTIME_PROFILE_MAJOR = 1 as const;
export const BASE_RUNTIME_PROFILE_MINOR = 2 as const;

export type BaseCapabilitySet = ReadonlyArray<number> & { readonly __maxItems: 64 };

export type BasePrerelease = string & { readonly __maxBytes: 32 };

export type CapabilitySetV1 = ReadonlyArray<number> & { readonly __maxItems: 64 };

export type CompatibilityDigestV1 = Uint8Array & { readonly __brand: "CompatibilityDigestV1" };

export type DatasetGenerationV1 = bigint & { readonly __brand: "DatasetGenerationV1" };

export type EventCursorV1 = bigint & { readonly __brand: "EventCursorV1" };

export type IdempotencyKeyV1 = Uint8Array & { readonly __brand: "IdempotencyKeyV1" };

export type LimitationCodeV1 = string & { readonly __maxBytes: 128 };

export type ManagementHandleV1 = Uint8Array & { readonly __brand: "ManagementHandleV1" };

export type MigrationVectorIdV1 = string & { readonly __maxBytes: 64 };

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

export type StorageSchemaVersion = number & { readonly __brand: "StorageSchemaVersion" };

export type SubscriptionHandleV1 = Uint8Array & { readonly __brand: "SubscriptionHandleV1" };

export type TargetTriple = string & { readonly __maxBytes: 96 };

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

export interface ArchiveRestorePolicyV1 {
  readonly canonical_schema_digest: CompatibilityDigestV1;
  readonly domain_registry_digest: CompatibilityDigestV1;
  readonly resource_registry_digest: CompatibilityDigestV1;
  readonly storage_schema: StorageSchemaVersion;
  readonly archive_profile: ProfileVersion;
  readonly migration_profile: ProfileVersion;
  readonly max_dataset_bytes: bigint;
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

export interface BaseCapabilityRequirements {
  readonly supported: BaseCapabilitySet;
  readonly required: BaseCapabilitySet;
}

export type BaseCommandV1 =
  | { readonly kind: 1; readonly name: "ExistingLocalCommand"; readonly payload: BaseLocalCommandV1 }
  | { readonly kind: 2; readonly name: "CreateArchive"; readonly payload: CreateArchiveCommandV1 }
  | { readonly kind: 3; readonly name: "RestoreArchive"; readonly payload: RestoreArchiveCommandV1 };

export enum BaseCompatibilityError {
  BaseMajorMismatch = 1,
  BaseMinorBelowMinimum = 2,
  CanonicalSchemaMismatch = 3,
  DomainRegistryMismatch = 4,
  ResourceRegistryMismatch = 5,
  RegistryProfileMismatch = 6,
  RegistryProfileDigestMismatch = 7,
  WireSessionMajorMismatch = 8,
  WireSessionMinorBelowMinimum = 9,
  ProductApiMajorMismatch = 10,
  ProductApiMinorBelowMinimum = 11,
  CAbiMajorMismatch = 12,
  CAbiMinorBelowMinimum = 13,
  MigrationVectorRequired = 14,
  MissingRequiredCapability = 15,
  InvalidPolicy = 16,
}

export interface BaseCompatibilityPolicy {
  readonly current: BaseCompatibilityTuple;
  readonly minimum_additive: NegotiatedVersions;
  readonly archive_restore: ArchiveRestorePolicyV1;
}

export interface BaseCompatibilityTuple {
  readonly base_version: BaseReleaseVersion;
  readonly base_commit: SourceCommitIdentity;
  readonly canonical_schema_digest: CompatibilityDigestV1;
  readonly domain_registry_digest: CompatibilityDigestV1;
  readonly resource_registry_digest: CompatibilityDigestV1;
  readonly storage_schema: StorageSchemaVersion;
  readonly archive_profile: ProfileVersion;
  readonly migration_profile: ProfileVersion;
  readonly registry_profile: ProfileVersion;
  readonly registry_profile_digest: CompatibilityDigestV1;
  readonly wire_session: ProfileVersion;
  readonly product_api: ProfileVersion;
  readonly c_abi: ProfileVersion;
  readonly feature_set_digest: CompatibilityDigestV1;
  readonly target_triple: TargetTriple;
  readonly toolchain: ToolchainIdentity;
}

export interface BaseCompatibleNegotiationV1 {
  readonly versions: NegotiatedVersions;
  readonly capabilities: BaseCapabilitySet;
}

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

export interface BaseMigrationRequiredNegotiationV1 {
  readonly from: BaseReleaseVersion;
  readonly to: BaseReleaseVersion;
  readonly vector: MigrationVectorBindingV1;
}

export type BaseNegotiationOutcome =
  | { readonly kind: 1; readonly name: "Compatible"; readonly payload: BaseCompatibleNegotiationV1 }
  | { readonly kind: 2; readonly name: "MigrationRequired"; readonly payload: BaseMigrationRequiredNegotiationV1 }
  | { readonly kind: 3; readonly name: "Incompatible"; readonly payload: BaseCompatibilityError };

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

export type BaseQualificationState =
  | { readonly kind: 1; readonly name: "Unqualified" }
  | { readonly kind: 2; readonly name: "Qualified"; readonly payload: BaseQualifiedEvidence };

export interface BaseQualifiedEvidence {
  readonly candidate_commit: SourceCommitId;
  readonly candidate_semantic_digest: CompatibilityDigestV1;
  readonly evidence_blake3: CompatibilityDigestV1;
}

export interface BaseQueryRequestV1 {
  readonly payload: TypedPayloadV1;
  readonly continuation?: BaseOpaqueContinuation;
  readonly budget: ResourceBudgetV1;
}

export interface BaseReleaseVersion {
  readonly major: number;
  readonly minor: number;
  readonly patch: number;
  readonly prerelease?: BasePrerelease;
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

export interface BaseVersionStatus {
  readonly compatibility: BaseCompatibilityTuple;
  readonly candidate_semantic_digest: CompatibilityDigestV1;
  readonly artifact_tuple_digest: CompatibilityDigestV1;
  readonly qualification: BaseQualificationState;
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

export enum KuDtoKindV1 {
  KuPrepareV1 = 19457,
  KuPreparedV1 = 19458,
  KuOperationRefV1 = 19459,
  KuSaveV1 = 19460,
  KuReceiptV1 = 19461,
  KuGetV1 = 19462,
  KuViewV1 = 19463,
  KuListV1 = 19464,
  KuSearchV1 = 19465,
  KuPageV1 = 19466,
  KuReviseV1 = 19467,
  KuExportV1 = 19468,
  KuExportViewV1 = 19469,
  KuStatusV1 = 19470,
  KuFailureV1 = 19471,
  KuStatusRequestV1 = 19472,
  KuPreparedArtifactV1 = 19473,
  KuSummaryV1 = 19474,
}

export enum KuOperationKindV1 {
  Prepare = 19201,
  Preview = 19202,
  Save = 19203,
  Get = 19204,
  List = 19205,
  Search = 19206,
  Revise = 19207,
  Export = 19208,
  Status = 19209,
  Cancel = 19210,
  Reconcile = 19211,
}

export interface MigrationVectorBindingV1 {
  readonly vector_id: MigrationVectorIdV1;
  readonly vector_blake3: CompatibilityDigestV1;
  readonly trust_policy_digest: CompatibilityDigestV1;
}

export interface NegotiatedVersions {
  readonly base_minor: number;
  readonly wire_session_minor: number;
  readonly product_api_minor: number;
  readonly c_abi_minor: number;
}

export type NodeTransportPublicIdV1 = Uint8Array & { readonly __brand: "NodeTransportPublicIdV1" };

export interface ProfileVersion {
  readonly major: number;
  readonly minor: number;
}

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

export type SourceCommitId =
  | { readonly kind: 1; readonly name: "Sha1"; readonly payload: SourceCommitSha1 }
  | { readonly kind: 2; readonly name: "Sha256"; readonly payload: SourceCommitSha256 };

export type SourceCommitIdentity =
  | { readonly kind: 1; readonly name: "Known"; readonly payload: SourceCommitId }
  | { readonly kind: 2; readonly name: "Unknown" };

export type SourceCommitSha1 = Uint8Array & { readonly __brand: "SourceCommitSha1" };

export type SourceCommitSha256 = Uint8Array & { readonly __brand: "SourceCommitSha256" };

export type ToolchainDigest = Uint8Array & { readonly __brand: "ToolchainDigest" };

export type ToolchainIdentity =
  | { readonly kind: 1; readonly name: "Known"; readonly payload: ToolchainDigest }
  | { readonly kind: 2; readonly name: "Unknown" };

export enum TopicKindV1 {
  RuntimeStatus = 1,
  OperationReceipts = 2,
  QueryResults = 3,
  ArchiveProgress = 4,
  Compatibility = 5,
}

// Registered KU local payloads; Base session and budget fences apply.
export type KuPayloadObjectCID = string & { readonly __role: "KuObjectCID" };
export type KuPayloadSemanticContentCID = string & { readonly __role: "KuSemanticContentCID" };
export type KuPayloadSourceArtifactCID = string & { readonly __role: "KuSourceArtifactCID" };
export type KuPayloadReleaseRoot = string & { readonly __role: "KuReleaseRoot" };
export type KuPayloadImplementationCommitment = string & { readonly __role: "KuImplementationCommitment" };
export type KuPayloadRevisionFrontier = string & { readonly __role: "KuRevisionFrontier" };
export type KuPayloadOperationId = string & { readonly __role: "KuOperationId" };
export type KuPayloadIdempotencyKey = string & { readonly __role: "KuIdempotencyKey" };
export type KuPayloadRequestId = string & { readonly __role: "KuRequestId" };
export type KuPayloadPolicyCID = string & { readonly __role: "KuPolicyCID" };
export type KuPayloadCCID = string & { readonly __role: "KuCCID" };
export type KuPayloadText = string;
export type KuPayloadLimitation = string;
export type KuPayloadContinuation = string;
export type KuPayloadU64 = bigint;
export type KuPayloadPageLimit = number;
export type KuPayloadCanonicalPreview = string;
export type KuPayloadDisclosure = "LOCAL_ONLY" | "NEGOTIATED_ENCRYPTED";
export type KuPayloadValidity = "ready" | "needs_resolution" | "rejected";
export type KuPayloadCoverage = "local_only" | "partial";
export type KuPayloadLifecycle = "disabled" | "requested" | "active" | "degraded";
export type KuPayloadBaseState = "reserved" | "prepared" | "confirming" | "committed" | "canceled" | "failed" | "unknown_outcome";
export type KuPayloadFalse = false;
export type KuPayloadBoolean = boolean;
export type KuPayloadBaseError = "InvalidRequest" | "NotFound" | "Conflict" | "Expired" | "RateLimited" | "CapabilityDisabled" | "DependencyUnavailable" | "IncompatibleProfile" | "ResourceExhausted" | "CorruptState" | "ReprovisionRequired" | "UnknownOutcome" | "InternalError";
export type KuPayloadExportMode = "canonical_public_exchange" | "encrypted_base_archive";
export type KuPayloadInputMode = "resolved_semantic_draft" | "local_rule" | "local_ai";
export type KuPayloadObjectIDs = ReadonlyArray<KuPayloadObjectCID>;
export type KuPayloadSources = ReadonlyArray<KuPayloadSourceArtifactCID>;
export type KuPayloadLimitations = ReadonlyArray<KuPayloadLimitation>;
export type KuPayloadKuViews = ReadonlyArray<KuPayloadKuSummaryV1>;
export type KuPayloadPreparedArtifacts = ReadonlyArray<KuPayloadKuPreparedArtifactV1>;
export type KuPayloadArtifactDisclosure = "PUBLIC" | "ROUTE_MINIMAL" | "LOCAL_ONLY" | "NEGOTIATED_ENCRYPTED";
export type KuPayloadArtifactValidity = "accepted_known" | "accepted_opaque";
export interface KuPayloadKuPrepareV1 {
  readonly operation_id: KuPayloadOperationId;
  readonly idempotency_key: KuPayloadIdempotencyKey;
  readonly input_mode: KuPayloadInputMode;
  readonly source_refs: KuPayloadSources;
  readonly registry_release_root: KuPayloadReleaseRoot;
  readonly semantic_profile: KuPayloadText;
  readonly implementation_commitment: KuPayloadImplementationCommitment;
  readonly destination: KuPayloadDisclosure;
  readonly draft_ref?: KuPayloadObjectCID;
}
export interface KuPayloadKuPreparedV1 {
  readonly operation_id: KuPayloadOperationId;
  readonly validity: KuPayloadValidity;
  readonly object_cids: KuPayloadObjectIDs;
  readonly registry_release_root: KuPayloadReleaseRoot;
  readonly semantic_profile: KuPayloadText;
  readonly destination: KuPayloadDisclosure;
  readonly limitations: KuPayloadLimitations;
  readonly executable: KuPayloadFalse;
  readonly artifacts: KuPayloadPreparedArtifacts;
}
export interface KuPayloadKuOperationRefV1 {
  readonly operation_id: KuPayloadOperationId;
}
export interface KuPayloadKuSaveV1 {
  readonly operation_id: KuPayloadOperationId;
  readonly idempotency_key: KuPayloadIdempotencyKey;
  readonly object_cids: KuPayloadObjectIDs;
}
export interface KuPayloadKuReceiptV1 {
  readonly operation_id: KuPayloadOperationId;
  readonly state: KuPayloadBaseState;
  readonly object_cids: KuPayloadObjectIDs;
  readonly limitations: KuPayloadLimitations;
  readonly published: KuPayloadFalse;
  readonly authorizes_reward: KuPayloadFalse;
}
export interface KuPayloadKuGetV1 {
  readonly object_cid: KuPayloadObjectCID;
}
export interface KuPayloadKuViewV1 {
  readonly object_cid: KuPayloadObjectCID;
  readonly disclosure_class: KuPayloadArtifactDisclosure;
  readonly artifact_validity: KuPayloadArtifactValidity;
  readonly coverage: KuPayloadCoverage;
  readonly limitations: KuPayloadLimitations;
  readonly executable: KuPayloadFalse;
  readonly canonical_bytes: KuPayloadCanonicalPreview;
  readonly semantic_content_cid?: KuPayloadSemanticContentCID;
  readonly fidelity_policy_cid?: KuPayloadPolicyCID;
  readonly fidelity_frontier?: KuPayloadRevisionFrontier;
}
export interface KuPayloadKuListV1 {
  readonly limit: KuPayloadPageLimit;
  readonly continuation?: KuPayloadContinuation;
}
export interface KuPayloadKuSearchV1 {
  readonly query: KuPayloadText;
  readonly limit: KuPayloadPageLimit;
  readonly continuation?: KuPayloadContinuation;
}
export interface KuPayloadKuPageV1 {
  readonly items: KuPayloadKuViews;
  readonly coverage: KuPayloadCoverage;
  readonly snapshot_frontier: KuPayloadRevisionFrontier;
  readonly limitations: KuPayloadLimitations;
  readonly continuation?: KuPayloadContinuation;
}
export interface KuPayloadKuReviseV1 {
  readonly preparation: KuPayloadKuPrepareV1;
  readonly predecessor_object_cid: KuPayloadObjectCID;
  readonly expected_revision_frontier: KuPayloadRevisionFrontier;
}
export interface KuPayloadKuExportV1 {
  readonly object_cids: KuPayloadObjectIDs;
  readonly mode: KuPayloadExportMode;
}
export interface KuPayloadKuExportViewV1 {
  readonly mode: KuPayloadExportMode;
  readonly object_cids: KuPayloadObjectIDs;
  readonly limitations: KuPayloadLimitations;
  readonly requires_base_management: KuPayloadBoolean;
  readonly public_records?: KuPayloadCanonicalPreview;
  readonly archive_operation_id?: KuPayloadOperationId;
}
export interface KuPayloadKuStatusV1 {
  readonly lifecycle: KuPayloadLifecycle;
  readonly coverage: KuPayloadCoverage;
  readonly limitations: KuPayloadLimitations;
  readonly registry_ready: KuPayloadBoolean;
  readonly local_encoder_ready: KuPayloadBoolean;
  readonly remote_encoding_enabled: KuPayloadFalse;
  readonly direct_issuance_enabled: KuPayloadFalse;
  readonly receipt?: KuPayloadKuReceiptV1;
}
export interface KuPayloadKuFailureV1 {
  readonly code: KuPayloadBaseError;
  readonly retryable: KuPayloadBoolean;
  readonly reconcile_before_retry: KuPayloadBoolean;
  readonly limitations: KuPayloadLimitations;
}
export interface KuPayloadKuStatusRequestV1 {
  readonly operation_id?: KuPayloadOperationId;
}
export interface KuPayloadKuPreparedArtifactV1 {
  readonly object_cid: KuPayloadObjectCID;
  readonly semantic_content_cid: KuPayloadSemanticContentCID;
  readonly canonical_preview: KuPayloadCanonicalPreview;
}
export interface KuPayloadKuSummaryV1 {
  readonly object_cid: KuPayloadObjectCID;
  readonly disclosure_class: KuPayloadArtifactDisclosure;
  readonly artifact_validity: KuPayloadArtifactValidity;
  readonly coverage: KuPayloadCoverage;
  readonly limitations: KuPayloadLimitations;
  readonly executable: KuPayloadFalse;
  readonly semantic_content_cid?: KuPayloadSemanticContentCID;
  readonly fidelity_policy_cid?: KuPayloadPolicyCID;
  readonly fidelity_frontier?: KuPayloadRevisionFrontier;
}
export const KU_OPERATION_IDS = {
  prepare: 19201,
  preview: 19202,
  save: 19203,
  get: 19204,
  list: 19205,
  search: 19206,
  revise: 19207,
  export: 19208,
  status: 19209,
  cancel: 19210,
  reconcile: 19211,
} as const;
export const KU_DTO_IDS = {"KuExportV1": 19468, "KuExportViewV1": 19469, "KuFailureV1": 19471, "KuGetV1": 19462, "KuListV1": 19464, "KuOperationRefV1": 19459, "KuPageV1": 19466, "KuPrepareV1": 19457, "KuPreparedArtifactV1": 19473, "KuPreparedV1": 19458, "KuReceiptV1": 19461, "KuReviseV1": 19467, "KuSaveV1": 19460, "KuSearchV1": 19465, "KuStatusRequestV1": 19472, "KuStatusV1": 19470, "KuSummaryV1": 19474, "KuViewV1": 19463} as const;
export const KU_PAYLOAD_SCHEMA = {"format":"onebrain/base-ku-payloads/1","minimum_base_minor":2,"encoding":"bounded_json_utf8","types":{"ObjectCID":{"kind":"hex","bytes":32,"role":"ObjectCID"},"SemanticContentCID":{"kind":"hex","bytes":32,"role":"SemanticContentCID"},"SourceArtifactCID":{"kind":"hex","bytes":32,"role":"SourceArtifactCID"},"ReleaseRoot":{"kind":"hex","bytes":32,"role":"ReleaseRoot"},"ImplementationCommitment":{"kind":"hex","bytes":32,"role":"ImplementationCommitment"},"RevisionFrontier":{"kind":"hex","bytes":32,"role":"RevisionFrontier"},"OperationId":{"kind":"hex","bytes":32,"role":"OperationId"},"IdempotencyKey":{"kind":"hex","bytes":32,"role":"IdempotencyKey"},"RequestId":{"kind":"hex","bytes":32,"role":"RequestId"},"PolicyCID":{"kind":"hex","bytes":32,"role":"PolicyCID"},"CCID":{"kind":"hex","bytes":16,"role":"CCID"},"Text":{"kind":"string","max_bytes":4096},"Limitation":{"kind":"string","max_bytes":128},"Continuation":{"kind":"string","max_bytes":2048},"U64":{"kind":"integer","min":0,"max":18446744073709551615},"PageLimit":{"kind":"integer","min":1,"max":256},"CanonicalPreview":{"kind":"base64","max_decoded_bytes":1048576},"Disclosure":{"kind":"enum","values":["LOCAL_ONLY","NEGOTIATED_ENCRYPTED"]},"Validity":{"kind":"enum","values":["ready","needs_resolution","rejected"]},"Coverage":{"kind":"enum","values":["local_only","partial"]},"Lifecycle":{"kind":"enum","values":["disabled","requested","active","degraded"]},"BaseState":{"kind":"enum","values":["reserved","prepared","confirming","committed","canceled","failed","unknown_outcome"]},"False":{"kind":"literal","value":false},"Boolean":{"kind":"boolean"},"BaseError":{"kind":"enum","values":["InvalidRequest","NotFound","Conflict","Expired","RateLimited","CapabilityDisabled","DependencyUnavailable","IncompatibleProfile","ResourceExhausted","CorruptState","ReprovisionRequired","UnknownOutcome","InternalError"]},"ExportMode":{"kind":"enum","values":["canonical_public_exchange","encrypted_base_archive"]},"InputMode":{"kind":"enum","values":["resolved_semantic_draft","local_rule","local_ai"]},"ObjectIDs":{"kind":"array","items":"ObjectCID","max_items":256},"Sources":{"kind":"array","items":"SourceArtifactCID","max_items":256},"Limitations":{"kind":"array","items":"Limitation","max_items":64},"KuViews":{"kind":"array","items":"KuSummaryV1","max_items":256},"PreparedArtifacts":{"kind":"array","items":"KuPreparedArtifactV1","max_items":256},"ArtifactDisclosure":{"kind":"enum","values":["PUBLIC","ROUTE_MINIMAL","LOCAL_ONLY","NEGOTIATED_ENCRYPTED"]},"ArtifactValidity":{"kind":"enum","values":["accepted_known","accepted_opaque"]}},"dtos":{"KuPrepareV1":{"required":{"operation_id":"OperationId","idempotency_key":"IdempotencyKey","input_mode":"InputMode","source_refs":"Sources","registry_release_root":"ReleaseRoot","semantic_profile":"Text","implementation_commitment":"ImplementationCommitment","destination":"Disclosure"},"optional":{"draft_ref":"ObjectCID"},"additional_fields":false},"KuPreparedV1":{"required":{"operation_id":"OperationId","validity":"Validity","object_cids":"ObjectIDs","registry_release_root":"ReleaseRoot","semantic_profile":"Text","destination":"Disclosure","limitations":"Limitations","executable":"False","artifacts":"PreparedArtifacts"},"optional":{},"additional_fields":false},"KuOperationRefV1":{"required":{"operation_id":"OperationId"},"optional":{},"additional_fields":false},"KuSaveV1":{"required":{"operation_id":"OperationId","idempotency_key":"IdempotencyKey","object_cids":"ObjectIDs"},"optional":{},"additional_fields":false},"KuReceiptV1":{"required":{"operation_id":"OperationId","state":"BaseState","object_cids":"ObjectIDs","limitations":"Limitations","published":"False","authorizes_reward":"False"},"optional":{},"additional_fields":false},"KuGetV1":{"required":{"object_cid":"ObjectCID"},"optional":{},"additional_fields":false},"KuViewV1":{"required":{"object_cid":"ObjectCID","disclosure_class":"ArtifactDisclosure","artifact_validity":"ArtifactValidity","coverage":"Coverage","limitations":"Limitations","executable":"False","canonical_bytes":"CanonicalPreview"},"optional":{"semantic_content_cid":"SemanticContentCID","fidelity_policy_cid":"PolicyCID","fidelity_frontier":"RevisionFrontier"},"additional_fields":false},"KuListV1":{"required":{"limit":"PageLimit"},"optional":{"continuation":"Continuation"},"additional_fields":false},"KuSearchV1":{"required":{"query":"Text","limit":"PageLimit"},"optional":{"continuation":"Continuation"},"additional_fields":false},"KuPageV1":{"required":{"items":"KuViews","coverage":"Coverage","snapshot_frontier":"RevisionFrontier","limitations":"Limitations"},"optional":{"continuation":"Continuation"},"additional_fields":false},"KuReviseV1":{"required":{"preparation":"KuPrepareV1","predecessor_object_cid":"ObjectCID","expected_revision_frontier":"RevisionFrontier"},"optional":{},"additional_fields":false},"KuExportV1":{"required":{"object_cids":"ObjectIDs","mode":"ExportMode"},"optional":{},"additional_fields":false},"KuExportViewV1":{"required":{"mode":"ExportMode","object_cids":"ObjectIDs","limitations":"Limitations","requires_base_management":"Boolean"},"optional":{"public_records":"CanonicalPreview","archive_operation_id":"OperationId"},"additional_fields":false},"KuStatusV1":{"required":{"lifecycle":"Lifecycle","coverage":"Coverage","limitations":"Limitations","registry_ready":"Boolean","local_encoder_ready":"Boolean","remote_encoding_enabled":"False","direct_issuance_enabled":"False"},"optional":{"receipt":"KuReceiptV1"},"additional_fields":false},"KuFailureV1":{"required":{"code":"BaseError","retryable":"Boolean","reconcile_before_retry":"Boolean","limitations":"Limitations"},"optional":{},"additional_fields":false},"KuStatusRequestV1":{"required":{},"optional":{"operation_id":"OperationId"},"additional_fields":false},"KuPreparedArtifactV1":{"required":{"object_cid":"ObjectCID","semantic_content_cid":"SemanticContentCID","canonical_preview":"CanonicalPreview"},"optional":{},"additional_fields":false},"KuSummaryV1":{"required":{"object_cid":"ObjectCID","disclosure_class":"ArtifactDisclosure","artifact_validity":"ArtifactValidity","coverage":"Coverage","limitations":"Limitations","executable":"False"},"optional":{"semantic_content_cid":"SemanticContentCID","fidelity_policy_cid":"PolicyCID","fidelity_frontier":"RevisionFrontier"},"additional_fields":false}},"dto_ids":{"KuPrepareV1":19457,"KuPreparedV1":19458,"KuOperationRefV1":19459,"KuSaveV1":19460,"KuReceiptV1":19461,"KuGetV1":19462,"KuViewV1":19463,"KuListV1":19464,"KuSearchV1":19465,"KuPageV1":19466,"KuReviseV1":19467,"KuExportV1":19468,"KuExportViewV1":19469,"KuStatusV1":19470,"KuFailureV1":19471,"KuStatusRequestV1":19472,"KuPreparedArtifactV1":19473,"KuSummaryV1":19474},"operations":[{"name":"prepare","base_boundary":"reserve_prepare","request":"KuPrepareV1","response":"KuPreparedV1","effect":"private_staging","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19201},{"name":"preview","base_boundary":"query","request":"KuOperationRefV1","response":"KuPreparedV1","effect":"none","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19202},{"name":"save","base_boundary":"confirm","request":"KuSaveV1","response":"KuReceiptV1","effect":"atomic_private_save","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19203},{"name":"get","base_boundary":"query","request":"KuGetV1","response":"KuViewV1","effect":"none","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19204},{"name":"list","base_boundary":"query","request":"KuListV1","response":"KuPageV1","effect":"none","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19205},{"name":"search","base_boundary":"query","request":"KuSearchV1","response":"KuPageV1","effect":"none","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19206},{"name":"revise","base_boundary":"reserve_prepare","request":"KuReviseV1","response":"KuPreparedV1","effect":"private_staging","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19207},{"name":"export","base_boundary":"query_or_CreateArchive","request":"KuExportV1","response":"KuExportViewV1","effect":"explicit_export","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19208},{"name":"status","base_boundary":"status","request":"KuStatusRequestV1","response":"KuStatusV1","effect":"none","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19209},{"name":"cancel","base_boundary":"cancel","request":"KuOperationRefV1","response":"KuReceiptV1","effect":"cancel_staging","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19210},{"name":"reconcile","base_boundary":"reconcile","request":"KuOperationRefV1","response":"KuReceiptV1","effect":"journal_recovery","visibility":"authenticated_local_private","surfaces":["node","rest","cli","web","desktop"],"wire_id":19211}],"errors":[{"id":1,"name":"InvalidRequest","retryable":false,"reconcile_before_retry":false,"rest_code":"invalid_request"},{"id":2,"name":"NotFound","retryable":false,"reconcile_before_retry":false,"rest_code":"not_found"},{"id":3,"name":"Conflict","retryable":false,"reconcile_before_retry":false,"rest_code":"conflict"},{"id":4,"name":"Expired","retryable":false,"reconcile_before_retry":false,"rest_code":"expired"},{"id":5,"name":"RateLimited","retryable":true,"reconcile_before_retry":true,"rest_code":"rate_limited"},{"id":6,"name":"CapabilityDisabled","retryable":false,"reconcile_before_retry":false,"rest_code":"capability_disabled"},{"id":7,"name":"DependencyUnavailable","retryable":true,"reconcile_before_retry":true,"rest_code":"dependency_unavailable"},{"id":8,"name":"IncompatibleProfile","retryable":false,"reconcile_before_retry":false,"rest_code":"conflict"},{"id":9,"name":"ResourceExhausted","retryable":true,"reconcile_before_retry":true,"rest_code":"rate_limited"},{"id":10,"name":"CorruptState","retryable":false,"reconcile_before_retry":false,"rest_code":"internal_error"},{"id":11,"name":"ReprovisionRequired","retryable":false,"reconcile_before_retry":false,"rest_code":"dependency_unavailable"},{"id":12,"name":"UnknownOutcome","retryable":true,"reconcile_before_retry":true,"rest_code":"dependency_unavailable"},{"id":13,"name":"InternalError","retryable":false,"reconcile_before_retry":true,"rest_code":"internal_error"}]} as const;
