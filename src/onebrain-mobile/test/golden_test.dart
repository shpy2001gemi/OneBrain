@Tags(<String>['golden'])
library;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:onebrain_mobile/app/onebrain_app.dart';
import 'package:onebrain_mobile/platform/generated/mobile_host_api.g.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUpAll(() async {
    await Future.wait(<Future<void>>[
      _loadFont('Nunito Sans', 'assets/fonts/NunitoSans-Variable.ttf'),
      _loadFont('Roboto Mono', 'assets/fonts/RobotoMono-Variable.ttf'),
      _loadFont(
        'Material Symbols Rounded',
        'assets/fonts/MaterialSymbolsRounded-Variable.ttf',
      ),
    ]);
  });

  final cases = <_GoldenCase>[
    const _GoldenCase(name: 'welcome_compact_light_en', size: Size(360, 800)),
    const _GoldenCase(
      name: 'welcome_large_dark_vi',
      size: Size(430, 932),
      brightness: Brightness.dark,
      locale: Locale('vi'),
    ),
    const _GoldenCase(
      name: 'welcome_compact_high_contrast_light_en',
      size: Size(360, 800),
      highContrast: true,
    ),
    const _GoldenCase(
      name: 'welcome_large_high_contrast_dark_en',
      size: Size(430, 932),
      brightness: Brightness.dark,
      highContrast: true,
    ),
    const _GoldenCase(
      name: 'welcome_large_text_200_en',
      size: Size(430, 932),
      textScale: 2,
    ),
    const _GoldenCase(
      name: 'welcome_large_reduced_motion_en',
      size: Size(430, 932),
      reducedMotion: true,
    ),
    const _GoldenCase(
      name: 'welcome_runtime_large_light_en',
      size: Size(430, 932),
      runtime: true,
    ),
    const _GoldenCase(
      name: 'gallery_large_light_en',
      size: Size(430, 932),
      gallery: true,
    ),
    const _GoldenCase(
      name: 'gallery_expanded_dark_vi',
      size: Size(900, 1180),
      brightness: Brightness.dark,
      locale: Locale('vi'),
      gallery: true,
    ),
    const _GoldenCase(
      name: 'home_limited_compact_light_en',
      size: Size(360, 800),
      home: true,
    ),
    const _GoldenCase(
      name: 'capture_text_large_light_en',
      size: Size(430, 932),
      textCapture: true,
    ),
    const _GoldenCase(
      name: 'share_spool_compact_light_en',
      size: Size(360, 800),
      shareSpool: true,
    ),
    const _GoldenCase(
      name: 'media_import_large_light_en',
      size: Size(430, 932),
      mediaImport: true,
    ),
    const _GoldenCase(
      name: 'my_media_large_light_en',
      size: Size(430, 932),
      myMedia: true,
    ),
  ];

  group('MOB-04 design-system golden matrix', () {
    for (final goldenCase in cases) {
      testWidgets(goldenCase.name, (tester) async {
        await _pumpGolden(tester, goldenCase);

        expect(tester.takeException(), isNull);
        await expectLater(
          find.byKey(_goldenBoundaryKey),
          matchesGoldenFile('goldens/${goldenCase.name}.png'),
        );
      });
    }
  });
}

const _goldenBoundaryKey = ValueKey<String>('mob04-golden-boundary');

Future<void> _pumpGolden(WidgetTester tester, _GoldenCase goldenCase) async {
  tester.view.physicalSize = goldenCase.size;
  tester.view.devicePixelRatio = 1;
  tester.platformDispatcher.platformBrightnessTestValue = goldenCase.brightness;
  tester.platformDispatcher.textScaleFactorTestValue = goldenCase.textScale;
  tester.platformDispatcher.accessibilityFeaturesTestValue =
      FakeAccessibilityFeatures(
        highContrast: goldenCase.highContrast,
        disableAnimations: goldenCase.reducedMotion,
        reduceMotion: goldenCase.reducedMotion,
      );
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);
  addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
  addTearDown(tester.platformDispatcher.clearAccessibilityFeaturesTestValue);

  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        mobileHostGatewayProvider.overrideWithValue(
          _FakeMobileHostGateway(
            onboardingCursor: goldenCase.shareSpool
                ? MobileOnboardingCursor.limitedHome
                : MobileOnboardingCursor.welcome,
            pendingSpools: goldenCase.shareSpool
                ? const [
                    MobileShareSpoolSummary(
                      spoolRef: 'spool_00000000000000000000000000000000',
                      mimeType: 'text/plain',
                      contentBytes: 41,
                      receivedAtMonotonicMillis: 7,
                    ),
                  ]
                : const [],
            ownedMedia: goldenCase.myMedia
                ? [
                    MobileOwnedMediaSummary(
                      mediaRef: 'media_${'a' * 64}',
                      mediaClass: MobileMediaClass.image,
                      mimeType: 'image/png',
                      contentBytes: 2048,
                      verifiedBytes: 2048,
                      storageClass: 'OwnedOriginal',
                      ownedHold: true,
                      importState: 'Complete',
                    ),
                  ]
                : const [],
          ),
        ),
      ],
      child: const RepaintBoundary(
        key: _goldenBoundaryKey,
        child: OneBrainApp(),
      ),
    ),
  );
  await tester.pumpAndSettle();

  if (goldenCase.locale.languageCode == 'vi') {
    await tester.tap(find.text('Tiếng Việt'));
    await tester.pumpAndSettle();
  }

  if (goldenCase.gallery) {
    final galleryAction = goldenCase.locale.languageCode == 'vi'
        ? find.text('Xem component dùng chung')
        : find.text('View shared components');
    await tester.scrollUntilVisible(galleryAction, 240);
    await tester.tap(galleryAction);
    await tester.pumpAndSettle();
  }

  if (goldenCase.runtime) {
    await tester.scrollUntilVisible(find.text('Mobile runtime profile'), 240);
    await tester.pumpAndSettle();
  }

  if (goldenCase.home ||
      goldenCase.textCapture ||
      goldenCase.mediaImport ||
      goldenCase.myMedia) {
    await _enterLimitedShell(tester, goldenCase.locale);
  }

  if (goldenCase.textCapture) {
    final captureAction = goldenCase.locale.languageCode == 'vi'
        ? 'Ghi văn bản'
        : 'Capture text';
    await _tapVisible(tester, captureAction);
  }

  if (goldenCase.mediaImport) {
    await _tapVisible(tester, 'Capture');
    await _tapVisible(tester, 'Import private media');
  }

  if (goldenCase.myMedia) {
    final libraryDestination = find.descendant(
      of: find.byType(NavigationBar),
      matching: find.text('Library'),
    );
    await tester.tap(libraryDestination);
    await tester.pumpAndSettle();

    final myMediaAction = find.widgetWithText(FilledButton, 'My media');
    await tester.ensureVisible(myMediaAction);
    await tester.pumpAndSettle();
    await tester.tap(myMediaAction);
    await tester.pumpAndSettle();
  }
}

Future<void> _enterLimitedShell(WidgetTester tester, Locale locale) async {
  final labels = locale.languageCode == 'vi'
      ? const <String>[
          'Tiếp tục kiểm tra thiết bị',
          'Tiếp',
          'Tiếp',
          'Tiếp',
          'Tạm dùng chế độ Giới hạn',
        ]
      : const <String>[
          'Continue to device preflight',
          'Next',
          'Next',
          'Next',
          'Use Limited mode for now',
        ];
  for (final label in labels) {
    await _tapVisible(tester, label);
  }
}

Future<void> _tapVisible(WidgetTester tester, String label) async {
  final target = find.text(label);
  await tester.ensureVisible(target);
  await tester.pumpAndSettle();
  await tester.tap(target);
  await tester.pumpAndSettle();
}

Future<void> _loadFont(String family, String asset) async {
  final loader = FontLoader(family)..addFont(rootBundle.load(asset));
  await loader.load();
}

class _GoldenCase {
  const _GoldenCase({
    required this.name,
    required this.size,
    this.brightness = Brightness.light,
    this.locale = const Locale('en'),
    this.highContrast = false,
    this.textScale = 1,
    this.reducedMotion = false,
    this.gallery = false,
    this.runtime = false,
    this.home = false,
    this.textCapture = false,
    this.shareSpool = false,
    this.mediaImport = false,
    this.myMedia = false,
  });

  final String name;
  final Size size;
  final Brightness brightness;
  final Locale locale;
  final bool highContrast;
  final double textScale;
  final bool reducedMotion;
  final bool gallery;
  final bool runtime;
  final bool home;
  final bool textCapture;
  final bool shareSpool;
  final bool mediaImport;
  final bool myMedia;
}

class _FakeMobileHostGateway implements MobileHostGateway {
  const _FakeMobileHostGateway({
    required this.onboardingCursor,
    required this.pendingSpools,
    required this.ownedMedia,
  });

  final MobileOnboardingCursor onboardingCursor;
  final List<MobileShareSpoolSummary> pendingSpools;
  final List<MobileOwnedMediaSummary> ownedMedia;

  @override
  Future<MobileHostSnapshot> inspectBootstrapHost() async =>
      const MobileHostSnapshot(
        platform: 'Android test',
        apiVersion: '8',
        registryRequestIssued: false,
        rustCoreLinked: true,
        rustCoreVersion: '0.1.0-test',
        rustAbiVersion: 8,
        rustRoundTripVerified: true,
      );

  @override
  Future<MobileRuntimeSnapshot> inspectRuntimeProfile() async =>
      MobileRuntimeSnapshot(
        profileVersion: 'MOB-04/3',
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
        stagedVerifiedMediaCount: 0,
        onboardingCursor: onboardingCursor,
      );

  @override
  Future<MobileRegistryInitAvailability>
  inspectRegistryInitAvailability() async =>
      const MobileRegistryInitAvailability(
        available: false,
        trustMode: MobileRegistryTrustMode.unavailable,
        channelId: 'stable',
        reasonCode: 'TEST_UNAVAILABLE',
        transportEnabled: false,
      );

  @override
  Future<MobileRegistryInitPlan> beginRegistryInit(String channelId) =>
      throw UnsupportedError('Registry Init is unavailable in this fake');

  @override
  Future<void> deferRegistryInit({
    required String operationId,
    required String manifestDigest,
  }) => throw UnsupportedError('Registry Init is unavailable in this fake');

  @override
  Future<MobileRegistryInitPlan> confirmRegistryInit({
    required String operationId,
    required String manifestDigest,
    required MobileRegistryNetworkPolicy networkPolicy,
    required bool oneTimeNetworkOverride,
  }) => throw UnsupportedError('Registry Init is unavailable in this fake');

  @override
  Future<MobileRegistryImportProgress> pickAndImportRegistryArtifact({
    required String operationId,
    required String manifestDigest,
    required MobileRegistryArtifactRole artifactRole,
  }) => throw UnsupportedError(
    'Registry Local Import is unavailable in this fake',
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
  Future<MobileOwnedMediaSummary> pickAndImportOwnedMedia(
    MobileMediaClass mediaClass,
  ) async => MobileOwnedMediaSummary(
    mediaRef: 'media_${'a' * 64}',
    mediaClass: mediaClass,
    mimeType: mediaClass == MobileMediaClass.document
        ? 'application/pdf'
        : '${mediaClass.name}/test',
    contentBytes: 32,
    verifiedBytes: 32,
    storageClass: 'OwnedOriginal',
    ownedHold: true,
    importState: 'Complete',
  );

  @override
  Future<List<MobileOwnedMediaSummary>> inspectOwnedMedia() async => ownedMedia;

  @override
  Future<String> startFeasibilityOperation(Duration delay) async =>
      'golden-operation';

  @override
  Future<bool> cancelFeasibilityOperation(String operationId) async => true;

  @override
  Stream<HostOperationEvent> observeFeasibilityOperations() =>
      const Stream<HostOperationEvent>.empty();
}
