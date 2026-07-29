import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:onebrain_mobile/app/locale_controller.dart';
import 'package:onebrain_mobile/app/onebrain_app.dart';
import 'package:onebrain_mobile/design/onebrain_theme.dart';
import 'package:onebrain_mobile/design/onebrain_theme_extensions.dart';
import 'package:onebrain_mobile/platform/generated/mobile_host_api.g.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  testWidgets('MOB-SCR-ENT-001 resolves to the English welcome screen', (
    tester,
  ) async {
    await tester.pumpWidget(_testApp());
    await tester.pumpAndSettle();

    expect(find.text('Grow ideas on your own node'), findsOneWidget);
    expect(find.textContaining('Android test host ready'), findsOneWidget);
    expect(find.text('Rust bridge 0.1.0-test · ABI 6'), findsOneWidget);
    expect(find.text('Typed round trip verified'), findsOneWidget);
    expect(
      find.text(
        'Protected identity, encrypted vault and local runtime verified',
      ),
      findsOneWidget,
    );
    expect(
      find.text(
        'No Registry artifact request is made from this bootstrap screen.',
      ),
      findsOneWidget,
    );
  });

  testWidgets('MOB-SCR-ONB-001 switches from English to Vietnamese', (
    tester,
  ) async {
    await tester.pumpWidget(_testApp());
    await tester.pumpAndSettle();

    await tester.tap(find.text('Tiếng Việt'));
    await tester.pumpAndSettle();

    expect(find.text('Nuôi dưỡng ý tưởng trên node của bạn'), findsOneWidget);
    expect(find.text('Chưa bắt đầu Registry Init'), findsOneWidget);
  });

  testWidgets(
    'MOB-04 onboarding reaches Limited shell and saves encrypted raw draft',
    (tester) async {
      Future<void> tapVisible(String label) async {
        final target = find.text(label);
        await tester.ensureVisible(target);
        await tester.pumpAndSettle();
        await tester.tap(target);
        await tester.pumpAndSettle();
      }

      await tester.pumpWidget(_testApp());
      await tester.pumpAndSettle();

      await tapVisible('Continue to device preflight');
      expect(find.text('Check the foundations'), findsOneWidget);
      await tapVisible('Next');
      expect(find.text('This installation is its own node'), findsOneWidget);
      await tapVisible('Next');
      expect(find.text('Private by default'), findsOneWidget);
      await tapVisible('Next');
      expect(
        find.text('Add required Concept data after launch'),
        findsOneWidget,
      );
      await tapVisible('Open required-data Init');
      expect(find.text('Required Concept data'), findsWidgets);
      await tapVisible('Use Limited mode for now');
      expect(find.text('A bright place for private ideas'), findsOneWidget);
      expect(find.text('Limited mode'), findsOneWidget);
      await tapVisible('Capture text');
      expect(find.text('Private text draft'), findsOneWidget);
      await tester.enterText(
        find.byType(TextField),
        'A private virtual-device idea',
      );
      await tapVisible('Save private draft');
      expect(find.text('Saved on this device'), findsOneWidget);
      expect(find.textContaining('1 draft'), findsOneWidget);
    },
  );

  testWidgets('shared components reflow at 200 percent text scale', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(360, 800);
    tester.view.devicePixelRatio = 1;
    tester.platformDispatcher.textScaleFactorTestValue = 2;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

    await tester.pumpWidget(_testApp());
    await tester.pumpAndSettle();
    final galleryAction = find.text('View shared components');
    await tester.ensureVisible(galleryAction);
    await tester.pumpAndSettle();
    await tester.tap(galleryAction);
    await tester.pumpAndSettle();

    expect(find.text('Shared component gallery'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Limited shell reflows at 320 pixels and 200 percent text', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(320, 568);
    tester.view.devicePixelRatio = 1;
    tester.platformDispatcher.textScaleFactorTestValue = 2;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);

    Future<void> tapVisible(String label) async {
      final target = find.text(label);
      await tester.ensureVisible(target);
      await tester.pumpAndSettle();
      await tester.tap(target);
      await tester.pumpAndSettle();
    }

    await tester.pumpWidget(_testApp());
    await tester.pumpAndSettle();
    await tapVisible('Continue to device preflight');
    await tapVisible('Next');
    await tapVisible('Next');
    await tapVisible('Next');
    await tapVisible('Use Limited mode for now');

    expect(find.text('Limited mode'), findsOneWidget);
    expect(find.text('Home'), findsWidgets);
    expect(find.text('Capture'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'MOB-SCR-CAP-003 previews opaque encrypted share spool before import',
    (tester) async {
      await tester.pumpWidget(
        _testApp(
          gateway: const _FakeMobileHostGateway(
            onboardingCursor: MobileOnboardingCursor.limitedHome,
            pendingSpools: [
              MobileShareSpoolSummary(
                spoolRef: 'spool_00000000000000000000000000000000',
                mimeType: 'text/plain',
                contentBytes: 41,
                receivedAtMonotonicMillis: 7,
              ),
            ],
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('Shared into OneBrain'), findsWidgets);
      expect(find.text('text/plain · 41 bytes'), findsOneWidget);
      expect(find.text('Import as private draft'), findsOneWidget);
      expect(find.textContaining('OneBrain emulator private'), findsNothing);

      await tester.tap(find.text('Import as private draft'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Shared text was imported'), findsOneWidget);
    },
  );

  test('theme projects generated token extensions', () {
    final theme = OneBrainTheme.light;

    expect(theme.extension<OneBrainSpacing>(), isNotNull);
    expect(theme.extension<OneBrainStatusColors>(), isNotNull);
    expect(theme.extension<OneBrainMotion>(), isNotNull);
    expect(theme.extension<OneBrainGradients>(), isNotNull);
    expect(theme.extension<OneBrainDataStyle>(), isNotNull);
    expect(theme.extension<OneBrainLayout>(), isNotNull);
  });

  test(
    'locale preference restores and persists through the package adapter',
    () async {
      final store = _MemoryLocalePreferenceStore('vi');
      final container = ProviderContainer(
        overrides: [localePreferenceStoreProvider.overrideWithValue(store)],
      );
      addTearDown(container.dispose);

      expect(container.read(localeControllerProvider), isNull);
      await Future<void>.delayed(Duration.zero);
      expect(container.read(localeControllerProvider), const Locale('vi'));

      container
          .read(localeControllerProvider.notifier)
          .select(const Locale('en'));
      await Future<void>.delayed(Duration.zero);
      expect(store.languageCode, 'en');
    },
  );

  testWidgets('reduced motion resolves all semantic durations to zero', (
    tester,
  ) async {
    late OneBrainMotion resolved;
    await tester.pumpWidget(
      MaterialApp(
        theme: OneBrainTheme.light,
        home: MediaQuery(
          data: const MediaQueryData(disableAnimations: true),
          child: Builder(
            builder: (context) {
              resolved = context.motion;
              return const SizedBox.shrink();
            },
          ),
        ),
      ),
    );

    expect(resolved.instant, Duration.zero);
    expect(resolved.press, Duration.zero);
    expect(resolved.micro, Duration.zero);
    expect(resolved.standard, Duration.zero);
    expect(resolved.emphasized, Duration.zero);
    expect(resolved.long, Duration.zero);
  });
}

Widget _testApp({MobileHostGateway gateway = const _FakeMobileHostGateway()}) =>
    ProviderScope(
      overrides: [mobileHostGatewayProvider.overrideWithValue(gateway)],
      child: const OneBrainApp(),
    );

class _FakeMobileHostGateway implements MobileHostGateway {
  const _FakeMobileHostGateway({
    this.onboardingCursor = MobileOnboardingCursor.welcome,
    this.pendingSpools = const [],
  });

  final MobileOnboardingCursor onboardingCursor;
  final List<MobileShareSpoolSummary> pendingSpools;

  @override
  Future<MobileHostSnapshot> inspectBootstrapHost() async =>
      const MobileHostSnapshot(
        platform: 'Android test',
        apiVersion: '6',
        registryRequestIssued: false,
        rustCoreLinked: true,
        rustCoreVersion: '0.1.0-test',
        rustAbiVersion: 6,
        rustRoundTripVerified: true,
      );

  @override
  Future<MobileRuntimeSnapshot> inspectRuntimeProfile() async =>
      MobileRuntimeSnapshot(
        profileVersion: 'MOB-04/2',
        processGeneration: 1,
        activationPhase: 'Active',
        activeGrantCount: 1,
        recoveredUncleanStart: false,
        bootstrapStoreOpened: true,
        registryState: 'BootstrapOnly',
        localKqlFixtureVerified: true,
        privatePlannerVerified: true,
        noLlmProvider: true,
        staleCallbackRejected: true,
        secureProfileActive: true,
        installationBindingVerified: true,
        installationCreated: true,
        securitySessionUnlocked: true,
        privateVaultReady: true,
        identityDomainsSeparated: true,
        privacyDefaultsFailSafe: true,
        redactedHistoryReady: true,
        encryptedRawDraftCount: 0,
        pendingShareSpoolCount: pendingSpools.length,
        onboardingCursor: onboardingCursor,
      );

  @override
  Future<void> setOnboardingCursor(MobileOnboardingCursor cursor) async {}

  @override
  Future<MobileRawDraftReceipt> saveRawTextDraft({
    required String contentLanguage,
    required String content,
  }) async => MobileRawDraftReceipt(
    draftRef: 'draft_00000000000000000000000000000000',
    contentLanguage: contentLanguage,
    contentBytes: content.length,
    totalDrafts: 1,
  );

  @override
  Future<List<MobileShareSpoolSummary>> inspectPendingShareSpools() async =>
      pendingSpools;

  @override
  Future<MobileRawDraftReceipt> importSharedText({
    required String spoolRef,
    required String contentLanguage,
  }) async =>
      saveRawTextDraft(contentLanguage: contentLanguage, content: 'shared');

  @override
  Future<String> startFeasibilityOperation(Duration delay) async =>
      'test-operation';

  @override
  Future<bool> cancelFeasibilityOperation(String operationId) async => true;

  @override
  Stream<HostOperationEvent> observeFeasibilityOperations() =>
      const Stream.empty();
}

class _MemoryLocalePreferenceStore implements LocalePreferenceStore {
  _MemoryLocalePreferenceStore(this.languageCode);

  String? languageCode;

  @override
  Future<String?> readLanguageCode() async => languageCode;

  @override
  Future<void> writeLanguageCode(String languageCode) async {
    this.languageCode = languageCode;
  }
}
