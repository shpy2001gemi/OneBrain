import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
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
    expect(find.text('Rust bridge 0.1.0-test · ABI 1'), findsOneWidget);
    expect(find.text('Typed round trip verified'), findsOneWidget);
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

  test('theme projects generated token extensions', () {
    final theme = OneBrainTheme.light;

    expect(theme.extension<OneBrainSpacing>(), isNotNull);
    expect(theme.extension<OneBrainStatusColors>(), isNotNull);
    expect(theme.extension<OneBrainMotion>(), isNotNull);
    expect(theme.extension<OneBrainGradients>(), isNotNull);
    expect(theme.extension<OneBrainDataStyle>(), isNotNull);
    expect(theme.extension<OneBrainLayout>(), isNotNull);
  });

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

Widget _testApp() => ProviderScope(
  overrides: [
    mobileHostGatewayProvider.overrideWithValue(const _FakeMobileHostGateway()),
  ],
  child: const OneBrainApp(),
);

class _FakeMobileHostGateway implements MobileHostGateway {
  const _FakeMobileHostGateway();

  @override
  Future<MobileHostSnapshot> inspectBootstrapHost() async =>
      const MobileHostSnapshot(
        platform: 'Android test',
        apiVersion: '1',
        registryRequestIssued: false,
        rustCoreLinked: true,
        rustCoreVersion: '0.1.0-test',
        rustAbiVersion: 1,
        rustRoundTripVerified: true,
      );

  @override
  Future<String> startFeasibilityOperation(Duration delay) async =>
      'test-operation';

  @override
  Future<bool> cancelFeasibilityOperation(String operationId) async => true;

  @override
  Stream<HostOperationEvent> observeFeasibilityOperations() =>
      const Stream.empty();
}
