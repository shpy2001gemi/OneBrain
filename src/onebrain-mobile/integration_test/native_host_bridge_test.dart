import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:onebrain_mobile/platform/generated/mobile_host_api.g.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'MOB-04 host bridge saves encrypted Limited-mode drafts and bounded operations',
    (tester) async {
      final gateway = PigeonMobileHostGateway();
      final events = gateway.observeFeasibilityOperations().asBroadcastStream();

      final snapshot = await gateway.inspectBootstrapHost();
      expect(snapshot.apiVersion, '6');
      expect(snapshot.registryRequestIssued, isFalse);
      expect(snapshot.rustCoreLinked, isTrue);
      expect(snapshot.rustAbiVersion, 6);
      expect(snapshot.rustRoundTripVerified, isTrue);

      final runtime = await gateway.inspectRuntimeProfile();
      expect(runtime.profileVersion, 'MOB-04/2');
      expect(runtime.pendingShareSpoolCount, greaterThanOrEqualTo(0));
      expect(runtime.processGeneration, greaterThanOrEqualTo(1));
      expect(runtime.activationPhase, 'Active');
      expect(runtime.activeGrantCount, 1);
      expect(runtime.bootstrapStoreOpened, isTrue);
      expect(runtime.registryState, 'BootstrapOnly');
      expect(runtime.localKqlFixtureVerified, isTrue);
      expect(runtime.privatePlannerVerified, isTrue);
      expect(runtime.noLlmProvider, isTrue);
      expect(runtime.staleCallbackRejected, isTrue);
      expect(runtime.secureProfileActive, isTrue);
      expect(runtime.installationBindingVerified, isTrue);
      expect(runtime.securitySessionUnlocked, isTrue);
      expect(runtime.privateVaultReady, isTrue);
      expect(runtime.identityDomainsSeparated, isTrue);
      expect(runtime.privacyDefaultsFailSafe, isTrue);
      expect(runtime.redactedHistoryReady, isTrue);
      expect(runtime.encryptedRawDraftCount, greaterThanOrEqualTo(0));
      await gateway.setOnboardingCursor(runtime.onboardingCursor);
      expect(
        (await gateway.inspectRuntimeProfile()).onboardingCursor,
        runtime.onboardingCursor,
      );

      const draftText = 'virtual-device private draft';
      final draft = await gateway.saveRawTextDraft(
        contentLanguage: 'en',
        content: draftText,
      );
      expect(draft.draftRef, matches(RegExp(r'^draft_[0-9a-f]{32}$')));
      expect(draft.contentLanguage, 'en');
      expect(draft.contentBytes, draftText.length);
      expect(draft.totalDrafts, runtime.encryptedRawDraftCount + 1);
      final afterDraft = await gateway.inspectRuntimeProfile();
      expect(afterDraft.encryptedRawDraftCount, draft.totalDrafts);

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
