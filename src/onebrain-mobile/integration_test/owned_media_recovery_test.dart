import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('MOB-MED-001 reopens complete OwnedOriginal after force-stop', (
    tester,
  ) async {
    final gateway = PigeonMobileHostGateway();
    final runtime = await gateway.inspectRuntimeProfile();
    expect(runtime.recoveredUncleanStart, isTrue);

    final shelf = await gateway.inspectOwnedMedia();
    expect(shelf, isNotEmpty);
    expect(
      shelf.every(
        (entry) =>
            entry.storageClass == 'OwnedOriginal' &&
            entry.importState == 'Complete' &&
            entry.ownedHold &&
            entry.verifiedBytes == entry.contentBytes,
      ),
      isTrue,
    );
  });
}
