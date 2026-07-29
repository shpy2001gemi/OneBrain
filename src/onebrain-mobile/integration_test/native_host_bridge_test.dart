import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:onebrain_mobile/platform/generated/mobile_host_api.g.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'MOB-01 typed host bridge supports async events cancellation and errors',
    (tester) async {
      final gateway = PigeonMobileHostGateway();
      final events = gateway.observeFeasibilityOperations().asBroadcastStream();

      final snapshot = await gateway.inspectBootstrapHost();
      expect(snapshot.apiVersion, '1');
      expect(snapshot.registryRequestIssued, isFalse);
      expect(snapshot.rustCoreLinked, isTrue);
      expect(snapshot.rustAbiVersion, 1);
      expect(snapshot.rustRoundTripVerified, isTrue);

      final startedForCancellation = events
          .firstWhere((event) => event.kind == HostOperationEventKind.started)
          .timeout(const Duration(seconds: 5));
      final cancellableId = await gateway.startFeasibilityOperation(
        const Duration(seconds: 10),
      );
      final started = await startedForCancellation;
      expect(started.operationId, cancellableId);
      expect(started.code, 'HOST_OPERATION_STARTED');

      final cancelledEvent = events
          .firstWhere(
            (event) =>
                event.operationId == cancellableId &&
                event.kind == HostOperationEventKind.cancelled,
          )
          .timeout(const Duration(seconds: 5));
      expect(await gateway.cancelFeasibilityOperation(cancellableId), isTrue);
      expect((await cancelledEvent).code, 'HOST_OPERATION_CANCELLED');
      expect(await gateway.cancelFeasibilityOperation(cancellableId), isFalse);

      final startedForCompletion = events
          .firstWhere((event) => event.kind == HostOperationEventKind.started)
          .timeout(const Duration(seconds: 5));
      final completedEvent = events
          .firstWhere((event) => event.kind == HostOperationEventKind.completed)
          .timeout(const Duration(seconds: 5));
      final completableId = await gateway.startFeasibilityOperation(
        const Duration(milliseconds: 25),
      );
      expect((await startedForCompletion).operationId, completableId);
      final completed = await completedEvent;
      expect(completed.operationId, completableId);
      expect(completed.code, 'HOST_OPERATION_COMPLETED');

      await expectLater(
        gateway.startFeasibilityOperation(const Duration(milliseconds: 30001)),
        throwsA(
          isA<PlatformException>().having(
            (error) => error.code,
            'code',
            'HOST_INVALID_DELAY',
          ),
        ),
      );
    },
  );
}
