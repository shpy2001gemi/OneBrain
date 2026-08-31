import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:test/test.dart';

import '../../../generated/dart/base_v1.dart';

Uint8List bytes(int fill) => Uint8List(32)..fillRange(0, 32, fill);

void main() {
  final corpus = jsonDecode(File('../corpus.json').readAsStringSync()) as Map<String, dynamic>;
  final reservation = BaseOperationReservationId(bytes(1));
  final operation = BaseOperationId(bytes(2));
  final subscription = BaseSubscriptionId(bytes(3));
  final capability = ArchiveCapabilityHandleV1(bytes(4));
  final source = ArchiveSourceHandleV1(bytes(5));
  final sink = ArchiveSinkHandleV1(bytes(6));
  final payload = TypedPayloadV1.tryFromBytes(Uint8List.fromList([1, 2, 3]));
  const budget = ResourceBudgetV1(max_items: 16, max_bytes: 4096, max_work_units: 1000);

  final ordinary = <BaseRequestV1>[
    const BaseRequestV1Status(),
    BaseRequestV1Query(BaseQueryRequestV1(payload: payload, budget: budget)),
    const BaseRequestV1ReserveOperation(BaseOperationKindV1.createArchive),
    BaseRequestV1Prepare(BasePrepareRequestV1(
      reservation_id: reservation,
      command: BaseCommandV1ExistingLocalCommand(BaseLocalCommandV1(kind: 7, payload: payload)),
    )),
    BaseRequestV1Confirm(BaseConfirmRequestV1(
      operation_id: operation,
      idempotency_key: BaseIdempotencyKey(bytes(9)),
    )),
    BaseRequestV1Cancel(operation),
    BaseRequestV1Reconcile(operation),
    const BaseRequestV1Subscribe(BaseSubscriptionRequestV1(
      topic: TopicKindV1.operationReceipts,
      cursor: 0,
    )),
    BaseRequestV1PollEvents(BasePollEventsRequestV1(
      subscription_id: subscription,
      after_cursor: 0,
      max_items: 16,
    )),
    BaseRequestV1CloseSubscription(subscription),
    const BaseRequestV1Drain(),
    const BaseRequestV1Close(),
  ];

  final management = <BaseManagementRequestV1>[
    BaseManagementRequestV1ArchiveSourceBegin(ArchiveSourceBeginV1(
      reservation_id: reservation,
      declared_total_bytes: 3,
    )),
    BaseManagementRequestV1ArchiveSourcePush(ArchiveSourcePushV1(
      handle: source,
      offset: 0,
      chunk: ArchiveChunkV1.tryFromBytes(Uint8List.fromList([1, 2, 3])),
    )),
    BaseManagementRequestV1ArchiveSourceSeal(capability),
    BaseManagementRequestV1ArchiveSinkBegin(ArchiveSinkBeginV1(
      reservation_id: reservation,
      max_total_bytes: 4096,
    )),
    BaseManagementRequestV1ArchiveSinkRead(ArchiveSinkReadV1(
      handle: sink,
      offset: 0,
      max_bytes: 4096,
    )),
    BaseManagementRequestV1ArchiveSinkCommit(capability),
    BaseManagementRequestV1ArchiveSecretRegister(BoundedSecretIngressV1(
      kind: ArchiveCredentialKindV1.password,
      bytes: Uint8List.fromList([1]),
    )),
    BaseManagementRequestV1ArchiveCapabilityAbort(capability),
    BaseManagementRequestV1ArchiveCapabilityDestroy(capability),
    BaseManagementRequestV1CompleteSignerReprovision(CompleteSignerReprovisionV1(
      domain: SignerDomainV1.nodeTransport,
      expected_public_id: SignerPublicIdV1NodeTransport(NodeTransportPublicIdV1(bytes(10))),
      provision_handle: SignerProvisionHandleV1(bytes(11)),
    )),
    const BaseManagementRequestV1Close(),
  ];

  test('generated discriminators match the one conformance corpus', () {
    expect(corpus['format'], 'onebrain/base-v1-projection-conformance/1');
    expect(
      ordinary.map((request) => request.discriminator).toList(),
      (corpus['ordinary'] as List<dynamic>)
          .map((entry) => (entry as Map<String, dynamic>)['id'])
          .toList(),
    );
    expect(
      management.map((request) => request.discriminator).toList(),
      (corpus['management'] as List<dynamic>)
          .map((entry) => (entry as Map<String, dynamic>)['id'])
          .toList(),
    );
    expect(
      BaseErrorCodeV1.values.map((error) => error.discriminator).toList(),
      (corpus['errors'] as List<dynamic>)
          .map((entry) => (entry as Map<String, dynamic>)['id'])
          .toList(),
    );
  });

  test('bounds and copied ownership are enforced outside Flutter', () {
    final input = Uint8List.fromList([7, 8]);
    final continuation = BaseOpaqueContinuation.tryFromBytes(input);
    input[0] = 0;
    expect(continuation.asBytes()[0], 7);
    expect(
      () => BaseOpaqueContinuation.tryFromBytes(Uint8List(4097)),
      throwsRangeError,
    );
    expect(
      (corpus['negative_vectors'] as List<dynamic>),
      contains('kill_reopen_unknown_outcome'),
    );
  });
}
