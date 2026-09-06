// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.
import 'dart:typed_data';

const int baseRuntimeProfileMajor = 1;
const int baseRuntimeProfileMinor = 2;

extension type const BaseCapabilitySet(List<int> value) {}

extension type const BasePrerelease(String value) {}

extension type const CapabilitySetV1(List<int> value) {}

final class CompatibilityDigestV1 {
  CompatibilityDigestV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

extension type const DatasetGenerationV1(int value) {}

extension type const EventCursorV1(int value) {}

final class IdempotencyKeyV1 {
  IdempotencyKeyV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

extension type const LimitationCodeV1(String value) {}

final class ManagementHandleV1 {
  ManagementHandleV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

extension type const MigrationVectorIdV1(String value) {}

final class OpaqueContinuationV1 {
  OpaqueContinuationV1._(this._value);

  final Uint8List _value;

  factory OpaqueContinuationV1.tryFromBytes(Uint8List bytes) {
    if (bytes.length > 4096) {
      throw RangeError('OpaqueContinuationV1 exceeds 4096 bytes');
    }
    return OpaqueContinuationV1._(Uint8List.fromList(bytes));
  }

  Uint8List asBytes() => Uint8List.fromList(_value);
}

final class OperationIdV1 {
  OperationIdV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class OperationReservationIdV1 {
  OperationReservationIdV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

extension type const ProcessGenerationV1(int value) {}

extension type const ProfileMajorV1(int value) {}

extension type const ProfileMinorV1(int value) {}

final class RequestIdV1 {
  RequestIdV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

extension type const StorageSchemaVersion(int value) {}

final class SubscriptionHandleV1 {
  SubscriptionHandleV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

extension type const TargetTriple(String value) {}

final class TypedPayloadV1 {
  TypedPayloadV1._(this._value);

  final Uint8List _value;

  factory TypedPayloadV1.tryFromBytes(Uint8List bytes) {
    if (bytes.length > 1048576) {
      throw RangeError('TypedPayloadV1 exceeds 1048576 bytes');
    }
    return TypedPayloadV1._(Uint8List.fromList(bytes));
  }

  Uint8List asBytes() => Uint8List.fromList(_value);
}

final class ActorRootPublicIdV1 {
  ActorRootPublicIdV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class ArchiveCapabilityHandleV1 {
  ArchiveCapabilityHandleV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class ArchiveChunkV1 {
  ArchiveChunkV1._(this._value);

  final Uint8List _value;

  factory ArchiveChunkV1.tryFromBytes(Uint8List bytes) {
    if (bytes.length > 1048576) {
      throw RangeError('ArchiveChunkV1 exceeds 1048576 bytes');
    }
    return ArchiveChunkV1._(Uint8List.fromList(bytes));
  }

  Uint8List asBytes() => Uint8List.fromList(_value);
}

enum ArchiveCredentialKindV1 {
  password(1),
  recoveryKey(2);

  const ArchiveCredentialKindV1(this.discriminator);
  final int discriminator;
}

final class ArchiveRestorePolicyV1 {
  const ArchiveRestorePolicyV1({
    required this.canonical_schema_digest,
    required this.domain_registry_digest,
    required this.resource_registry_digest,
    required this.storage_schema,
    required this.archive_profile,
    required this.migration_profile,
    required this.max_dataset_bytes,
  });

  final CompatibilityDigestV1 canonical_schema_digest;
  final CompatibilityDigestV1 domain_registry_digest;
  final CompatibilityDigestV1 resource_registry_digest;
  final StorageSchemaVersion storage_schema;
  final ProfileVersion archive_profile;
  final ProfileVersion migration_profile;
  final int max_dataset_bytes;
}

final class ArchiveSecretHandleV1 {
  ArchiveSecretHandleV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class ArchiveSinkBeginV1 {
  const ArchiveSinkBeginV1({
    required this.reservation_id,
    required this.max_total_bytes,
  });

  final BaseOperationReservationId reservation_id;
  final int max_total_bytes;
}

final class ArchiveSinkHandleV1 {
  ArchiveSinkHandleV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class ArchiveSinkReadV1 {
  const ArchiveSinkReadV1({
    required this.handle,
    required this.offset,
    required this.max_bytes,
  });

  final ArchiveSinkHandleV1 handle;
  final int offset;
  final int max_bytes;
}

final class ArchiveSourceBeginV1 {
  const ArchiveSourceBeginV1({
    required this.reservation_id,
    required this.declared_total_bytes,
  });

  final BaseOperationReservationId reservation_id;
  final int declared_total_bytes;
}

final class ArchiveSourceHandleV1 {
  ArchiveSourceHandleV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class ArchiveSourcePushV1 {
  const ArchiveSourcePushV1({
    required this.handle,
    required this.offset,
    required this.chunk,
  });

  final ArchiveSourceHandleV1 handle;
  final int offset;
  final ArchiveChunkV1 chunk;
}

final class BaseCapabilityRequirements {
  const BaseCapabilityRequirements({
    required this.supported,
    required this.required,
  });

  final BaseCapabilitySet supported;
  final BaseCapabilitySet required;
}

sealed class BaseCommandV1 {
  const BaseCommandV1(this.discriminator);
  final int discriminator;
}

final class BaseCommandV1ExistingLocalCommand extends BaseCommandV1 {
  const BaseCommandV1ExistingLocalCommand(this.payload) : super(1);
  final BaseLocalCommandV1 payload;
}

final class BaseCommandV1CreateArchive extends BaseCommandV1 {
  const BaseCommandV1CreateArchive(this.payload) : super(2);
  final CreateArchiveCommandV1 payload;
}

final class BaseCommandV1RestoreArchive extends BaseCommandV1 {
  const BaseCommandV1RestoreArchive(this.payload) : super(3);
  final RestoreArchiveCommandV1 payload;
}

enum BaseCompatibilityError {
  baseMajorMismatch(1),
  baseMinorBelowMinimum(2),
  canonicalSchemaMismatch(3),
  domainRegistryMismatch(4),
  resourceRegistryMismatch(5),
  registryProfileMismatch(6),
  registryProfileDigestMismatch(7),
  wireSessionMajorMismatch(8),
  wireSessionMinorBelowMinimum(9),
  productApiMajorMismatch(10),
  productApiMinorBelowMinimum(11),
  cAbiMajorMismatch(12),
  cAbiMinorBelowMinimum(13),
  migrationVectorRequired(14),
  missingRequiredCapability(15),
  invalidPolicy(16);

  const BaseCompatibilityError(this.discriminator);
  final int discriminator;
}

final class BaseCompatibilityPolicy {
  const BaseCompatibilityPolicy({
    required this.current,
    required this.minimum_additive,
    required this.archive_restore,
  });

  final BaseCompatibilityTuple current;
  final NegotiatedVersions minimum_additive;
  final ArchiveRestorePolicyV1 archive_restore;
}

final class BaseCompatibilityTuple {
  const BaseCompatibilityTuple({
    required this.base_version,
    required this.base_commit,
    required this.canonical_schema_digest,
    required this.domain_registry_digest,
    required this.resource_registry_digest,
    required this.storage_schema,
    required this.archive_profile,
    required this.migration_profile,
    required this.registry_profile,
    required this.registry_profile_digest,
    required this.wire_session,
    required this.product_api,
    required this.c_abi,
    required this.feature_set_digest,
    required this.target_triple,
    required this.toolchain,
  });

  final BaseReleaseVersion base_version;
  final SourceCommitIdentity base_commit;
  final CompatibilityDigestV1 canonical_schema_digest;
  final CompatibilityDigestV1 domain_registry_digest;
  final CompatibilityDigestV1 resource_registry_digest;
  final StorageSchemaVersion storage_schema;
  final ProfileVersion archive_profile;
  final ProfileVersion migration_profile;
  final ProfileVersion registry_profile;
  final CompatibilityDigestV1 registry_profile_digest;
  final ProfileVersion wire_session;
  final ProfileVersion product_api;
  final ProfileVersion c_abi;
  final CompatibilityDigestV1 feature_set_digest;
  final TargetTriple target_triple;
  final ToolchainIdentity toolchain;
}

final class BaseCompatibleNegotiationV1 {
  const BaseCompatibleNegotiationV1({
    required this.versions,
    required this.capabilities,
  });

  final NegotiatedVersions versions;
  final BaseCapabilitySet capabilities;
}

final class BaseConfirmRequestV1 {
  const BaseConfirmRequestV1({
    required this.operation_id,
    required this.idempotency_key,
  });

  final BaseOperationId operation_id;
  final BaseIdempotencyKey idempotency_key;
}

enum BaseErrorCodeV1 {
  invalidRequest(1),
  notFound(2),
  conflict(3),
  expired(4),
  rateLimited(5),
  capabilityDisabled(6),
  dependencyUnavailable(7),
  incompatibleProfile(8),
  resourceExhausted(9),
  corruptState(10),
  reprovisionRequired(11),
  unknownOutcome(12),
  internalError(13);

  const BaseErrorCodeV1(this.discriminator);
  final int discriminator;
}

final class BaseIdempotencyKey {
  BaseIdempotencyKey(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class BaseLocalCommandV1 {
  const BaseLocalCommandV1({
    required this.kind,
    required this.payload,
  });

  final int kind;
  final TypedPayloadV1 payload;
}

final class BaseManagementGrantV1 {
  BaseManagementGrantV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

sealed class BaseManagementRequestV1 {
  const BaseManagementRequestV1(this.discriminator);
  final int discriminator;
}

final class BaseManagementRequestV1ArchiveSourceBegin extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveSourceBegin(this.payload) : super(102);
  final ArchiveSourceBeginV1 payload;
}

final class BaseManagementRequestV1ArchiveSourcePush extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveSourcePush(this.payload) : super(103);
  final ArchiveSourcePushV1 payload;
}

final class BaseManagementRequestV1ArchiveSourceSeal extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveSourceSeal(this.payload) : super(104);
  final ArchiveCapabilityHandleV1 payload;
}

final class BaseManagementRequestV1ArchiveSinkBegin extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveSinkBegin(this.payload) : super(105);
  final ArchiveSinkBeginV1 payload;
}

final class BaseManagementRequestV1ArchiveSinkRead extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveSinkRead(this.payload) : super(106);
  final ArchiveSinkReadV1 payload;
}

final class BaseManagementRequestV1ArchiveSinkCommit extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveSinkCommit(this.payload) : super(107);
  final ArchiveCapabilityHandleV1 payload;
}

final class BaseManagementRequestV1ArchiveSecretRegister extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveSecretRegister(this.payload) : super(108);
  final BoundedSecretIngressV1 payload;
}

final class BaseManagementRequestV1ArchiveCapabilityAbort extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveCapabilityAbort(this.payload) : super(109);
  final ArchiveCapabilityHandleV1 payload;
}

final class BaseManagementRequestV1ArchiveCapabilityDestroy extends BaseManagementRequestV1 {
  const BaseManagementRequestV1ArchiveCapabilityDestroy(this.payload) : super(110);
  final ArchiveCapabilityHandleV1 payload;
}

final class BaseManagementRequestV1CompleteSignerReprovision extends BaseManagementRequestV1 {
  const BaseManagementRequestV1CompleteSignerReprovision(this.payload) : super(111);
  final CompleteSignerReprovisionV1 payload;
}

final class BaseManagementRequestV1Close extends BaseManagementRequestV1 {
  const BaseManagementRequestV1Close() : super(112);
}

final class BaseMigrationRequiredNegotiationV1 {
  const BaseMigrationRequiredNegotiationV1({
    required this.from,
    required this.to,
    required this.vector,
  });

  final BaseReleaseVersion from;
  final BaseReleaseVersion to;
  final MigrationVectorBindingV1 vector;
}

sealed class BaseNegotiationOutcome {
  const BaseNegotiationOutcome(this.discriminator);
  final int discriminator;
}

final class BaseNegotiationOutcomeCompatible extends BaseNegotiationOutcome {
  const BaseNegotiationOutcomeCompatible(this.payload) : super(1);
  final BaseCompatibleNegotiationV1 payload;
}

final class BaseNegotiationOutcomeMigrationRequired extends BaseNegotiationOutcome {
  const BaseNegotiationOutcomeMigrationRequired(this.payload) : super(2);
  final BaseMigrationRequiredNegotiationV1 payload;
}

final class BaseNegotiationOutcomeIncompatible extends BaseNegotiationOutcome {
  const BaseNegotiationOutcomeIncompatible(this.payload) : super(3);
  final BaseCompatibilityError payload;
}

final class BaseOpaqueContinuation {
  BaseOpaqueContinuation._(this._value);

  final Uint8List _value;

  factory BaseOpaqueContinuation.tryFromBytes(Uint8List bytes) {
    if (bytes.length > 4096) {
      throw RangeError('BaseOpaqueContinuation exceeds 4096 bytes');
    }
    return BaseOpaqueContinuation._(Uint8List.fromList(bytes));
  }

  Uint8List asBytes() => Uint8List.fromList(_value);
}

final class BaseOperationId {
  BaseOperationId(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

enum BaseOperationKindV1 {
  existingLocalCommand(1),
  createArchive(2),
  restoreArchive(3);

  const BaseOperationKindV1(this.discriminator);
  final int discriminator;
}

final class BaseOperationReservationId {
  BaseOperationReservationId(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class BasePollEventsRequestV1 {
  const BasePollEventsRequestV1({
    required this.subscription_id,
    required this.after_cursor,
    required this.max_items,
  });

  final BaseSubscriptionId subscription_id;
  final int after_cursor;
  final int max_items;
}

final class BasePrepareRequestV1 {
  const BasePrepareRequestV1({
    required this.reservation_id,
    required this.command,
  });

  final BaseOperationReservationId reservation_id;
  final BaseCommandV1 command;
}

sealed class BaseQualificationState {
  const BaseQualificationState(this.discriminator);
  final int discriminator;
}

final class BaseQualificationStateUnqualified extends BaseQualificationState {
  const BaseQualificationStateUnqualified() : super(1);
}

final class BaseQualificationStateQualified extends BaseQualificationState {
  const BaseQualificationStateQualified(this.payload) : super(2);
  final BaseQualifiedEvidence payload;
}

final class BaseQualifiedEvidence {
  const BaseQualifiedEvidence({
    required this.candidate_commit,
    required this.candidate_semantic_digest,
    required this.evidence_blake3,
  });

  final SourceCommitId candidate_commit;
  final CompatibilityDigestV1 candidate_semantic_digest;
  final CompatibilityDigestV1 evidence_blake3;
}

final class BaseQueryRequestV1 {
  const BaseQueryRequestV1({
    required this.payload,
    this.continuation,
    required this.budget,
  });

  final TypedPayloadV1 payload;
  final BaseOpaqueContinuation? continuation;
  final ResourceBudgetV1 budget;
}

final class BaseReleaseVersion {
  const BaseReleaseVersion({
    required this.major,
    required this.minor,
    required this.patch,
    this.prerelease,
  });

  final int major;
  final int minor;
  final int patch;
  final BasePrerelease? prerelease;
}

sealed class BaseRequestV1 {
  const BaseRequestV1(this.discriminator);
  final int discriminator;
}

final class BaseRequestV1Status extends BaseRequestV1 {
  const BaseRequestV1Status() : super(3);
}

final class BaseRequestV1Query extends BaseRequestV1 {
  const BaseRequestV1Query(this.payload) : super(5);
  final BaseQueryRequestV1 payload;
}

final class BaseRequestV1ReserveOperation extends BaseRequestV1 {
  const BaseRequestV1ReserveOperation(this.payload) : super(6);
  final BaseOperationKindV1 payload;
}

final class BaseRequestV1Prepare extends BaseRequestV1 {
  const BaseRequestV1Prepare(this.payload) : super(7);
  final BasePrepareRequestV1 payload;
}

final class BaseRequestV1Confirm extends BaseRequestV1 {
  const BaseRequestV1Confirm(this.payload) : super(8);
  final BaseConfirmRequestV1 payload;
}

final class BaseRequestV1Cancel extends BaseRequestV1 {
  const BaseRequestV1Cancel(this.payload) : super(9);
  final BaseOperationId payload;
}

final class BaseRequestV1Reconcile extends BaseRequestV1 {
  const BaseRequestV1Reconcile(this.payload) : super(10);
  final BaseOperationId payload;
}

final class BaseRequestV1Subscribe extends BaseRequestV1 {
  const BaseRequestV1Subscribe(this.payload) : super(11);
  final BaseSubscriptionRequestV1 payload;
}

final class BaseRequestV1PollEvents extends BaseRequestV1 {
  const BaseRequestV1PollEvents(this.payload) : super(12);
  final BasePollEventsRequestV1 payload;
}

final class BaseRequestV1CloseSubscription extends BaseRequestV1 {
  const BaseRequestV1CloseSubscription(this.payload) : super(13);
  final BaseSubscriptionId payload;
}

final class BaseRequestV1Drain extends BaseRequestV1 {
  const BaseRequestV1Drain() : super(14);
}

final class BaseRequestV1Close extends BaseRequestV1 {
  const BaseRequestV1Close() : super(15);
}

final class BaseSubscriptionId {
  BaseSubscriptionId(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class BaseSubscriptionRequestV1 {
  const BaseSubscriptionRequestV1({
    required this.topic,
    this.cursor,
  });

  final TopicKindV1 topic;
  final int? cursor;
}

final class BaseVersionStatus {
  const BaseVersionStatus({
    required this.compatibility,
    required this.candidate_semantic_digest,
    required this.artifact_tuple_digest,
    required this.qualification,
  });

  final BaseCompatibilityTuple compatibility;
  final CompatibilityDigestV1 candidate_semantic_digest;
  final CompatibilityDigestV1 artifact_tuple_digest;
  final BaseQualificationState qualification;
}

final class BoundedSecretIngressV1 {
  const BoundedSecretIngressV1({
    required this.kind,
    required this.bytes,
  });

  final ArchiveCredentialKindV1 kind;
  final Uint8List bytes;
}

final class CompleteSignerReprovisionV1 {
  const CompleteSignerReprovisionV1({
    required this.domain,
    required this.expected_public_id,
    required this.provision_handle,
  });

  final SignerDomainV1 domain;
  final SignerPublicIdV1 expected_public_id;
  final SignerProvisionHandleV1 provision_handle;
}

final class CreateArchiveCommandV1 {
  const CreateArchiveCommandV1({
    required this.sink,
    required this.secret,
    required this.budget,
  });

  final ArchiveSinkHandleV1 sink;
  final ArchiveSecretHandleV1 secret;
  final ResourceBudgetV1 budget;
}

final class FeedAuthorPublicIdV1 {
  FeedAuthorPublicIdV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

enum KuDtoKindV1 {
  kuPrepareV1(19457),
  kuPreparedV1(19458),
  kuOperationRefV1(19459),
  kuSaveV1(19460),
  kuReceiptV1(19461),
  kuGetV1(19462),
  kuViewV1(19463),
  kuListV1(19464),
  kuSearchV1(19465),
  kuPageV1(19466),
  kuReviseV1(19467),
  kuExportV1(19468),
  kuExportViewV1(19469),
  kuStatusV1(19470),
  kuFailureV1(19471),
  kuStatusRequestV1(19472),
  kuPreparedArtifactV1(19473),
  kuSummaryV1(19474);

  const KuDtoKindV1(this.discriminator);
  final int discriminator;
}

enum KuOperationKindV1 {
  prepare(19201),
  preview(19202),
  save(19203),
  get(19204),
  list(19205),
  search(19206),
  revise(19207),
  export(19208),
  status(19209),
  cancel(19210),
  reconcile(19211);

  const KuOperationKindV1(this.discriminator);
  final int discriminator;
}

final class MigrationVectorBindingV1 {
  const MigrationVectorBindingV1({
    required this.vector_id,
    required this.vector_blake3,
    required this.trust_policy_digest,
  });

  final MigrationVectorIdV1 vector_id;
  final CompatibilityDigestV1 vector_blake3;
  final CompatibilityDigestV1 trust_policy_digest;
}

final class NegotiatedVersions {
  const NegotiatedVersions({
    required this.base_minor,
    required this.wire_session_minor,
    required this.product_api_minor,
    required this.c_abi_minor,
  });

  final int base_minor;
  final int wire_session_minor;
  final int product_api_minor;
  final int c_abi_minor;
}

final class NodeTransportPublicIdV1 {
  NodeTransportPublicIdV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class ProfileVersion {
  const ProfileVersion({
    required this.major,
    required this.minor,
  });

  final int major;
  final int minor;
}

final class ResourceBudgetV1 {
  const ResourceBudgetV1({
    required this.max_items,
    required this.max_bytes,
    required this.max_work_units,
  });

  final int max_items;
  final int max_bytes;
  final int max_work_units;
}

final class RestoreArchiveCommandV1 {
  const RestoreArchiveCommandV1({
    required this.source,
    required this.secret,
    required this.budget,
  });

  final ArchiveSourceHandleV1 source;
  final ArchiveSecretHandleV1 secret;
  final ResourceBudgetV1 budget;
}

enum SignerDomainV1 {
  nodeTransport(1),
  actorRoot(2),
  feedAuthor(3);

  const SignerDomainV1(this.discriminator);
  final int discriminator;
}

final class SignerProvisionHandleV1 {
  SignerProvisionHandleV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

sealed class SignerPublicIdV1 {
  const SignerPublicIdV1(this.discriminator);
  final int discriminator;
}

final class SignerPublicIdV1NodeTransport extends SignerPublicIdV1 {
  const SignerPublicIdV1NodeTransport(this.payload) : super(1);
  final NodeTransportPublicIdV1 payload;
}

final class SignerPublicIdV1ActorRoot extends SignerPublicIdV1 {
  const SignerPublicIdV1ActorRoot(this.payload) : super(2);
  final ActorRootPublicIdV1 payload;
}

final class SignerPublicIdV1FeedAuthor extends SignerPublicIdV1 {
  const SignerPublicIdV1FeedAuthor(this.payload) : super(3);
  final FeedAuthorPublicIdV1 payload;
}

sealed class SourceCommitId {
  const SourceCommitId(this.discriminator);
  final int discriminator;
}

final class SourceCommitIdSha1 extends SourceCommitId {
  const SourceCommitIdSha1(this.payload) : super(1);
  final SourceCommitSha1 payload;
}

final class SourceCommitIdSha256 extends SourceCommitId {
  const SourceCommitIdSha256(this.payload) : super(2);
  final SourceCommitSha256 payload;
}

sealed class SourceCommitIdentity {
  const SourceCommitIdentity(this.discriminator);
  final int discriminator;
}

final class SourceCommitIdentityKnown extends SourceCommitIdentity {
  const SourceCommitIdentityKnown(this.payload) : super(1);
  final SourceCommitId payload;
}

final class SourceCommitIdentityUnknown extends SourceCommitIdentity {
  const SourceCommitIdentityUnknown() : super(2);
}

final class SourceCommitSha1 {
  SourceCommitSha1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class SourceCommitSha256 {
  SourceCommitSha256(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

final class ToolchainDigest {
  ToolchainDigest(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

sealed class ToolchainIdentity {
  const ToolchainIdentity(this.discriminator);
  final int discriminator;
}

final class ToolchainIdentityKnown extends ToolchainIdentity {
  const ToolchainIdentityKnown(this.payload) : super(1);
  final ToolchainDigest payload;
}

final class ToolchainIdentityUnknown extends ToolchainIdentity {
  const ToolchainIdentityUnknown() : super(2);
}

enum TopicKindV1 {
  runtimeStatus(1),
  operationReceipts(2),
  queryResults(3),
  archiveProgress(4),
  compatibility(5);

  const TopicKindV1(this.discriminator);
  final int discriminator;
}

// Registered KU local payload declarations.
typedef KuPayloadObjectCID = String;
typedef KuPayloadSemanticContentCID = String;
typedef KuPayloadSourceArtifactCID = String;
typedef KuPayloadReleaseRoot = String;
typedef KuPayloadImplementationCommitment = String;
typedef KuPayloadRevisionFrontier = String;
typedef KuPayloadOperationId = String;
typedef KuPayloadIdempotencyKey = String;
typedef KuPayloadRequestId = String;
typedef KuPayloadPolicyCID = String;
typedef KuPayloadCCID = String;
typedef KuPayloadText = String;
typedef KuPayloadLimitation = String;
typedef KuPayloadContinuation = String;
typedef KuPayloadU64 = int;
typedef KuPayloadPageLimit = int;
typedef KuPayloadCanonicalPreview = String;
enum KuPayloadDisclosure { vLOCALONLY("LOCAL_ONLY"), vNEGOTIATEDENCRYPTED("NEGOTIATED_ENCRYPTED");
const KuPayloadDisclosure(this.wireValue); final String wireValue;
}
enum KuPayloadValidity { vReady("ready"), vNeedsResolution("needs_resolution"), vRejected("rejected");
const KuPayloadValidity(this.wireValue); final String wireValue;
}
enum KuPayloadCoverage { vLocalOnly("local_only"), vPartial("partial");
const KuPayloadCoverage(this.wireValue); final String wireValue;
}
enum KuPayloadLifecycle { vDisabled("disabled"), vRequested("requested"), vActive("active"), vDegraded("degraded");
const KuPayloadLifecycle(this.wireValue); final String wireValue;
}
enum KuPayloadBaseState { vReserved("reserved"), vPrepared("prepared"), vConfirming("confirming"), vCommitted("committed"), vCanceled("canceled"), vFailed("failed"), vUnknownOutcome("unknown_outcome");
const KuPayloadBaseState(this.wireValue); final String wireValue;
}
typedef KuPayloadFalse = bool;
typedef KuPayloadBoolean = bool;
enum KuPayloadBaseError { vInvalidRequest("InvalidRequest"), vNotFound("NotFound"), vConflict("Conflict"), vExpired("Expired"), vRateLimited("RateLimited"), vCapabilityDisabled("CapabilityDisabled"), vDependencyUnavailable("DependencyUnavailable"), vIncompatibleProfile("IncompatibleProfile"), vResourceExhausted("ResourceExhausted"), vCorruptState("CorruptState"), vReprovisionRequired("ReprovisionRequired"), vUnknownOutcome("UnknownOutcome"), vInternalError("InternalError");
const KuPayloadBaseError(this.wireValue); final String wireValue;
}
enum KuPayloadExportMode { vCanonicalPublicExchange("canonical_public_exchange"), vEncryptedBaseArchive("encrypted_base_archive");
const KuPayloadExportMode(this.wireValue); final String wireValue;
}
enum KuPayloadInputMode { vResolvedSemanticDraft("resolved_semantic_draft"), vLocalRule("local_rule"), vLocalAi("local_ai");
const KuPayloadInputMode(this.wireValue); final String wireValue;
}
typedef KuPayloadObjectIDs = List<KuPayloadObjectCID>;
typedef KuPayloadSources = List<KuPayloadSourceArtifactCID>;
typedef KuPayloadLimitations = List<KuPayloadLimitation>;
typedef KuPayloadKuViews = List<KuPayloadKuSummaryV1>;
typedef KuPayloadPreparedArtifacts = List<KuPayloadKuPreparedArtifactV1>;
enum KuPayloadArtifactDisclosure { vPUBLIC("PUBLIC"), vROUTEMINIMAL("ROUTE_MINIMAL"), vLOCALONLY("LOCAL_ONLY"), vNEGOTIATEDENCRYPTED("NEGOTIATED_ENCRYPTED");
const KuPayloadArtifactDisclosure(this.wireValue); final String wireValue;
}
enum KuPayloadArtifactValidity { vAcceptedKnown("accepted_known"), vAcceptedOpaque("accepted_opaque");
const KuPayloadArtifactValidity(this.wireValue); final String wireValue;
}
final class KuPayloadKuPrepareV1 {
const KuPayloadKuPrepareV1({
required this.operation_id,
required this.idempotency_key,
required this.input_mode,
required this.source_refs,
required this.registry_release_root,
required this.semantic_profile,
required this.implementation_commitment,
required this.destination,
this.draft_ref,
});
final KuPayloadOperationId operation_id;
final KuPayloadIdempotencyKey idempotency_key;
final KuPayloadInputMode input_mode;
final KuPayloadSources source_refs;
final KuPayloadReleaseRoot registry_release_root;
final KuPayloadText semantic_profile;
final KuPayloadImplementationCommitment implementation_commitment;
final KuPayloadDisclosure destination;
final KuPayloadObjectCID? draft_ref;
}
final class KuPayloadKuPreparedV1 {
const KuPayloadKuPreparedV1({
required this.operation_id,
required this.validity,
required this.object_cids,
required this.registry_release_root,
required this.semantic_profile,
required this.destination,
required this.limitations,
required this.executable,
required this.artifacts,
});
final KuPayloadOperationId operation_id;
final KuPayloadValidity validity;
final KuPayloadObjectIDs object_cids;
final KuPayloadReleaseRoot registry_release_root;
final KuPayloadText semantic_profile;
final KuPayloadDisclosure destination;
final KuPayloadLimitations limitations;
final KuPayloadFalse executable;
final KuPayloadPreparedArtifacts artifacts;
}
final class KuPayloadKuOperationRefV1 {
const KuPayloadKuOperationRefV1({
required this.operation_id,
});
final KuPayloadOperationId operation_id;
}
final class KuPayloadKuSaveV1 {
const KuPayloadKuSaveV1({
required this.operation_id,
required this.idempotency_key,
required this.object_cids,
});
final KuPayloadOperationId operation_id;
final KuPayloadIdempotencyKey idempotency_key;
final KuPayloadObjectIDs object_cids;
}
final class KuPayloadKuReceiptV1 {
const KuPayloadKuReceiptV1({
required this.operation_id,
required this.state,
required this.object_cids,
required this.limitations,
required this.published,
required this.authorizes_reward,
});
final KuPayloadOperationId operation_id;
final KuPayloadBaseState state;
final KuPayloadObjectIDs object_cids;
final KuPayloadLimitations limitations;
final KuPayloadFalse published;
final KuPayloadFalse authorizes_reward;
}
final class KuPayloadKuGetV1 {
const KuPayloadKuGetV1({
required this.object_cid,
});
final KuPayloadObjectCID object_cid;
}
final class KuPayloadKuViewV1 {
const KuPayloadKuViewV1({
required this.object_cid,
required this.disclosure_class,
required this.artifact_validity,
required this.coverage,
required this.limitations,
required this.executable,
required this.canonical_bytes,
this.semantic_content_cid,
this.fidelity_policy_cid,
this.fidelity_frontier,
});
final KuPayloadObjectCID object_cid;
final KuPayloadArtifactDisclosure disclosure_class;
final KuPayloadArtifactValidity artifact_validity;
final KuPayloadCoverage coverage;
final KuPayloadLimitations limitations;
final KuPayloadFalse executable;
final KuPayloadCanonicalPreview canonical_bytes;
final KuPayloadSemanticContentCID? semantic_content_cid;
final KuPayloadPolicyCID? fidelity_policy_cid;
final KuPayloadRevisionFrontier? fidelity_frontier;
}
final class KuPayloadKuListV1 {
const KuPayloadKuListV1({
required this.limit,
this.continuation,
});
final KuPayloadPageLimit limit;
final KuPayloadContinuation? continuation;
}
final class KuPayloadKuSearchV1 {
const KuPayloadKuSearchV1({
required this.query,
required this.limit,
this.continuation,
});
final KuPayloadText query;
final KuPayloadPageLimit limit;
final KuPayloadContinuation? continuation;
}
final class KuPayloadKuPageV1 {
const KuPayloadKuPageV1({
required this.items,
required this.coverage,
required this.snapshot_frontier,
required this.limitations,
this.continuation,
});
final KuPayloadKuViews items;
final KuPayloadCoverage coverage;
final KuPayloadRevisionFrontier snapshot_frontier;
final KuPayloadLimitations limitations;
final KuPayloadContinuation? continuation;
}
final class KuPayloadKuReviseV1 {
const KuPayloadKuReviseV1({
required this.preparation,
required this.predecessor_object_cid,
required this.expected_revision_frontier,
});
final KuPayloadKuPrepareV1 preparation;
final KuPayloadObjectCID predecessor_object_cid;
final KuPayloadRevisionFrontier expected_revision_frontier;
}
final class KuPayloadKuExportV1 {
const KuPayloadKuExportV1({
required this.object_cids,
required this.mode,
});
final KuPayloadObjectIDs object_cids;
final KuPayloadExportMode mode;
}
final class KuPayloadKuExportViewV1 {
const KuPayloadKuExportViewV1({
required this.mode,
required this.object_cids,
required this.limitations,
required this.requires_base_management,
this.public_records,
this.archive_operation_id,
});
final KuPayloadExportMode mode;
final KuPayloadObjectIDs object_cids;
final KuPayloadLimitations limitations;
final KuPayloadBoolean requires_base_management;
final KuPayloadCanonicalPreview? public_records;
final KuPayloadOperationId? archive_operation_id;
}
final class KuPayloadKuStatusV1 {
const KuPayloadKuStatusV1({
required this.lifecycle,
required this.coverage,
required this.limitations,
required this.registry_ready,
required this.local_encoder_ready,
required this.remote_encoding_enabled,
required this.direct_issuance_enabled,
this.receipt,
});
final KuPayloadLifecycle lifecycle;
final KuPayloadCoverage coverage;
final KuPayloadLimitations limitations;
final KuPayloadBoolean registry_ready;
final KuPayloadBoolean local_encoder_ready;
final KuPayloadFalse remote_encoding_enabled;
final KuPayloadFalse direct_issuance_enabled;
final KuPayloadKuReceiptV1? receipt;
}
final class KuPayloadKuFailureV1 {
const KuPayloadKuFailureV1({
required this.code,
required this.retryable,
required this.reconcile_before_retry,
required this.limitations,
});
final KuPayloadBaseError code;
final KuPayloadBoolean retryable;
final KuPayloadBoolean reconcile_before_retry;
final KuPayloadLimitations limitations;
}
final class KuPayloadKuStatusRequestV1 {
const KuPayloadKuStatusRequestV1({
this.operation_id,
});
final KuPayloadOperationId? operation_id;
}
final class KuPayloadKuPreparedArtifactV1 {
const KuPayloadKuPreparedArtifactV1({
required this.object_cid,
required this.semantic_content_cid,
required this.canonical_preview,
});
final KuPayloadObjectCID object_cid;
final KuPayloadSemanticContentCID semantic_content_cid;
final KuPayloadCanonicalPreview canonical_preview;
}
final class KuPayloadKuSummaryV1 {
const KuPayloadKuSummaryV1({
required this.object_cid,
required this.disclosure_class,
required this.artifact_validity,
required this.coverage,
required this.limitations,
required this.executable,
this.semantic_content_cid,
this.fidelity_policy_cid,
this.fidelity_frontier,
});
final KuPayloadObjectCID object_cid;
final KuPayloadArtifactDisclosure disclosure_class;
final KuPayloadArtifactValidity artifact_validity;
final KuPayloadCoverage coverage;
final KuPayloadLimitations limitations;
final KuPayloadFalse executable;
final KuPayloadSemanticContentCID? semantic_content_cid;
final KuPayloadPolicyCID? fidelity_policy_cid;
final KuPayloadRevisionFrontier? fidelity_frontier;
}
const kuOperationIds = <String, int>{
'prepare': 19201,
'preview': 19202,
'save': 19203,
'get': 19204,
'list': 19205,
'search': 19206,
'revise': 19207,
'export': 19208,
'status': 19209,
'cancel': 19210,
'reconcile': 19211,
};
const kuDtoIds = <String, int>{"KuPrepareV1": 19457, "KuPreparedV1": 19458, "KuOperationRefV1": 19459, "KuSaveV1": 19460, "KuReceiptV1": 19461, "KuGetV1": 19462, "KuViewV1": 19463, "KuListV1": 19464, "KuSearchV1": 19465, "KuPageV1": 19466, "KuReviseV1": 19467, "KuExportV1": 19468, "KuExportViewV1": 19469, "KuStatusV1": 19470, "KuFailureV1": 19471, "KuStatusRequestV1": 19472, "KuPreparedArtifactV1": 19473, "KuSummaryV1": 19474};
