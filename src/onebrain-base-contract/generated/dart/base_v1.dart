// Generated from src/test-vectors/vnext/base-v1-runtime-interface-v1.json; DO NOT EDIT.
import 'dart:typed_data';

const int baseRuntimeProfileMajor = 1;
const int baseRuntimeProfileMinor = 0;

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

final class SubscriptionHandleV1 {
  SubscriptionHandleV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
}

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

final class NodeTransportPublicIdV1 {
  NodeTransportPublicIdV1(Uint8List value) : value = Uint8List.fromList(value);
  final Uint8List value;
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

enum TopicKindV1 {
  runtimeStatus(1),
  operationReceipts(2),
  queryResults(3),
  archiveProgress(4),
  compatibility(5);

  const TopicKindV1(this.discriminator);
  final int discriminator;
}
