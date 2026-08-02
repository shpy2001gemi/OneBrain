import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'MOB-05B imports all signed Registry roles through the Android picker',
    (tester) async {
      final gateway = PigeonMobileHostGateway();
      final host = await gateway.inspectBootstrapHost();
      expect(host.apiVersion, '9');
      expect(host.rustAbiVersion, 13);

      final availability = await gateway.inspectRegistryInitAvailability();
      expect(availability.available, isTrue);
      var plan = await gateway.beginRegistryInit(availability.channelId);
      if (plan.stateCode == 5) {
        plan = await gateway.confirmRegistryInit(
          operationId: plan.operationId,
          manifestDigest: plan.manifestDigest,
          networkPolicy: MobileRegistryNetworkPolicy.unmetered,
          oneTimeNetworkOverride: false,
        );
      }
      expect(plan.stateCode, 8);

      MobileRegistryImportProgress? progress;
      for (final role in MobileRegistryArtifactRole.values) {
        progress = await gateway.pickAndImportRegistryArtifact(
          operationId: plan.operationId,
          manifestDigest: plan.manifestDigest,
          artifactRole: role,
        );
        expect(progress.selectedRole, role);
        expect(progress.roleComplete, isTrue);
        expect(progress.sourcePlanDigest, matches(RegExp(r'^[0-9a-f]{64}$')));
        expect(progress.totalChunks, 3);
        expect(progress.expectedBytes, 6144);
      }

      expect(progress, isNotNull);
      expect(progress!.verifiedChunks, 3);
      expect(progress.verifiedBytes, 6144);
      expect(progress.bytesComplete, isTrue);
    },
  );
}
