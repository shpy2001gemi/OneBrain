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
  ];

  group('MOB-03 design-system golden matrix', () {
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

const _goldenBoundaryKey = ValueKey<String>('mob02-golden-boundary');

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
          const _FakeMobileHostGateway(),
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
}

class _FakeMobileHostGateway implements MobileHostGateway {
  const _FakeMobileHostGateway();

  @override
  Future<MobileHostSnapshot> inspectBootstrapHost() async =>
      const MobileHostSnapshot(
        platform: 'Android test',
        apiVersion: '3',
        registryRequestIssued: false,
        rustCoreLinked: true,
        rustCoreVersion: '0.1.0-test',
        rustAbiVersion: 3,
        rustRoundTripVerified: true,
      );

  @override
  Future<MobileRuntimeSnapshot> inspectRuntimeProfile() async =>
      const MobileRuntimeSnapshot(
        profileVersion: 'MOB-03/1',
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
      );

  @override
  Future<String> startFeasibilityOperation(Duration delay) async =>
      'golden-operation';

  @override
  Future<bool> cancelFeasibilityOperation(String operationId) async => true;

  @override
  Stream<HostOperationEvent> observeFeasibilityOperations() =>
      const Stream<HostOperationEvent>.empty();
}
