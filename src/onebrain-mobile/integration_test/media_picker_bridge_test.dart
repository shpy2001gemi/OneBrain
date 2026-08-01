import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('MOB-MED-001 imports one Android picker image as OwnedOriginal', (
    tester,
  ) async {
    final gateway = PigeonMobileHostGateway();
    final before = await gateway.inspectRuntimeProfile();
    final shelfBefore = await gateway.inspectOwnedMedia();

    final receipt = await gateway.pickAndImportOwnedMedia(
      MobileMediaClass.image,
    );
    expect(receipt.mediaRef, matches(RegExp(r'^media_[0-9a-f]{64}$')));
    expect(receipt.mediaClass, MobileMediaClass.image);
    expect(receipt.mimeType, 'image/png');
    expect(receipt.contentBytes, greaterThan(0));
    expect(receipt.verifiedBytes, receipt.contentBytes);
    expect(receipt.storageClass, 'OwnedOriginal');
    expect(receipt.ownedHold, isTrue);
    expect(receipt.importState, 'Complete');

    final after = await gateway.inspectRuntimeProfile();
    expect(after.stagedVerifiedMediaCount, before.stagedVerifiedMediaCount);
    final shelfAfter = await gateway.inspectOwnedMedia();
    expect(shelfAfter.length, greaterThanOrEqualTo(shelfBefore.length));
    expect(
      shelfAfter.any(
        (entry) =>
            entry.mediaRef == receipt.mediaRef &&
            entry.storageClass == 'OwnedOriginal' &&
            entry.ownedHold,
      ),
      isTrue,
    );
  });
}
