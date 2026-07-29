import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('MOB-CAP-002 imports one Android share spool idempotently', (
    tester,
  ) async {
    final gateway = PigeonMobileHostGateway();
    final before = await gateway.inspectRuntimeProfile();
    expect(before.pendingShareSpoolCount, greaterThanOrEqualTo(1));

    final spools = await gateway.inspectPendingShareSpools();
    expect(spools.length, before.pendingShareSpoolCount);
    final spool = spools.first;
    expect(spool.spoolRef, matches(RegExp(r'^spool_[0-9a-f]{32}$')));
    expect(spool.mimeType, 'text/plain');
    expect(spool.contentBytes, greaterThan(0));

    final imported = await gateway.importSharedText(
      spoolRef: spool.spoolRef,
      contentLanguage: 'en',
    );
    expect(imported.draftRef, matches(RegExp(r'^draft_[0-9a-f]{32}$')));
    expect(imported.contentBytes, spool.contentBytes);
    expect(imported.totalDrafts, before.encryptedRawDraftCount + 1);

    final retry = await gateway.importSharedText(
      spoolRef: spool.spoolRef,
      contentLanguage: 'en',
    );
    expect(retry.draftRef, imported.draftRef);
    expect(retry.totalDrafts, imported.totalDrafts);

    final after = await gateway.inspectRuntimeProfile();
    expect(after.pendingShareSpoolCount, before.pendingShareSpoolCount - 1);
    expect(after.encryptedRawDraftCount, imported.totalDrafts);
  });
}
