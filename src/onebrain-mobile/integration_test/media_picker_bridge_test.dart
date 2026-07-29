import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('MOB-CAP-003 stages one Android system-picker image', (
    tester,
  ) async {
    final gateway = PigeonMobileHostGateway();
    final before = await gateway.inspectRuntimeProfile();

    final receipt = await gateway.pickAndStagePrivateMedia(
      MobileMediaClass.image,
    );
    expect(receipt.sourceRef, matches(RegExp(r'^source_[0-9a-f]{32}$')));
    expect(receipt.mediaClass, MobileMediaClass.image);
    expect(receipt.mimeType, 'image/png');
    expect(receipt.contentBytes, greaterThan(0));
    expect(receipt.blake3Digest, matches(RegExp(r'^[0-9a-f]{64}$')));

    final after = await gateway.inspectRuntimeProfile();
    expect(
      after.stagedVerifiedMediaCount,
      before.stagedVerifiedMediaCount + 1,
    );
  });
}
