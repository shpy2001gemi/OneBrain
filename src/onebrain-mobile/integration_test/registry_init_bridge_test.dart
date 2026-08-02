import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'MOB-05A verifies signed debug metadata and binds exact admission',
    (tester) async {
      final gateway = PigeonMobileHostGateway();
      final host = await gateway.inspectBootstrapHost();
      expect(host.apiVersion, '9');
      expect(host.rustAbiVersion, 13);

      final availability = await gateway.inspectRegistryInitAvailability();
      expect(availability.available, isTrue);
      expect(
        availability.trustMode,
        MobileRegistryTrustMode.developmentFixture,
      );
      expect(availability.channelId, 'stable');
      expect(availability.transportEnabled, isFalse);

      var plan = await gateway.beginRegistryInit(availability.channelId);
      expect(plan.channelId, 'stable');
      expect(plan.releaseId, matches(RegExp(r'^[0-9a-f]{64}$')));
      expect(plan.manifestDigest, matches(RegExp(r'^[0-9a-f]{64}$')));
      expect(plan.trustProfileDigest, matches(RegExp(r'^[0-9a-f]{64}$')));
      expect(plan.publisherMinAdditionalFreeBytes, 2000000000);
      expect(plan.artifactTotalBytes, 6144);
      expect(plan.targetTotalAllocBytes, greaterThanOrEqualTo(6144));
      expect(plan.initialRequiredFreeBytes, greaterThanOrEqualTo(2000000000));
      expect(plan.transportEnabled, isFalse);

      if (plan.stateCode == 5) {
        await gateway.deferRegistryInit(
          operationId: plan.operationId,
          manifestDigest: plan.manifestDigest,
        );
        plan = await gateway.beginRegistryInit(availability.channelId);
        expect(plan.stateCode, 5);
        plan = await gateway.confirmRegistryInit(
          operationId: plan.operationId,
          manifestDigest: plan.manifestDigest,
          networkPolicy: MobileRegistryNetworkPolicy.unmetered,
          oneTimeNetworkOverride: false,
        );
      }

      expect(plan.stateCode, plan.admitted ? 8 : 21);
      expect(plan.transportEnabled, isFalse);
      expect(
        (await gateway.inspectBootstrapHost()).registryRequestIssued,
        isTrue,
      );
    },
  );
}
