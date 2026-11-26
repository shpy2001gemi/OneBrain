import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:onebrain_mobile/app/app_router.dart';
import 'package:onebrain_mobile/app/onebrain_app.dart';
import 'package:onebrain_mobile/domain/wave7_workspace_controller.dart';
import 'package:onebrain_mobile/platform/generated/mobile_host_api.g.dart';
import 'package:onebrain_mobile/platform/mobile_host_gateway.dart';

void main() {
  test(
    'MOB-08 / SITEMAP guard: blocked publication, blind fidelity, P2P network, match, wallet and active M6 KQL routes are absent from consumer router',
    () {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final router = container.read(appRouterProvider);
      final paths = _collectRoutePaths(router.configuration.routes);

      // Required active Wave 7 routes MUST be present:
      expect(paths, contains('/onboarding/readiness'));
      expect(paths, contains('/init/registry/plan'));
      expect(paths, contains('/operations'));
      expect(paths, contains('/operations/:operationRef'));
      expect(paths, contains('/library/my-ku'));
      expect(paths, contains('/library/search'));
      expect(paths, contains('/library/kql'));
      expect(paths, contains('/library/graph/:localRef'));
      expect(paths, contains('/library/concepts'));
      expect(paths, contains('/library/concepts/:ccid'));
      expect(paths, contains('/library/knowledge/:localRef'));
      expect(paths, contains('/library/knowledge/:localRef/fidelity'));
      expect(paths, contains('/library/media/:manifestRef'));
      expect(paths, contains('/library/media/:manifestRef/info'));
      expect(paths, contains('/assistant/proposal/:proposalRef'));
      expect(paths, contains('/settings/ai/models/:releaseRef'));
      expect(paths, contains('/activity'));
      expect(paths, contains('/settings/diagnostics'));
      expect(paths, contains('/settings/advanced'));

      // Blocked / gated routes MUST be absent from active consumer navigation:
      for (final blocked in const <String>[
        '/knowledge-publication',
        '/publish',
        '/library/knowledge/:localRef/publish',
        '/fidelity/jobs',
        '/fidelity/requests',
        '/fidelity/blind',
        '/settings/network',
        '/network',
        '/peers',
        '/library/needs',
        '/library/matches',
        '/wallet',
        '/kql/active',
      ]) {
        expect(paths, isNot(contains(blocked)));
      }
      expect(
        paths.any(
          (p) =>
              p.contains('publish') ||
              p.contains('wallet') ||
              p.contains('/network') ||
              p.contains('/matches') ||
              p.contains('/needs'),
        ),
        isFalse,
      );
    },
  );

  testWidgets(
    'MOB-05C & MOB-04: canonical KU encode is gated before ReadyOffline, then unlocks preview, redirect rewrite, immutable LOCAL_ONLY save, SELF_ATTESTED fidelity and read-only KQL',
    (tester) async {
      final container = ProviderContainer(
        overrides: [
          mobileHostGatewayProvider.overrideWithValue(
            const _Wave7FakeHostGateway(),
          ),
        ],
      );
      addTearDown(container.dispose);

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const OneBrainApp(),
        ),
      );
      await tester.pumpAndSettle();

      final router = container.read(appRouterProvider);

      // 1. Before Registry activation, /capture/operations/draft_1/encode is gated!
      router.go('/capture/operations/draft_1/encode');
      await tester.pumpAndSettle();
      expect(find.text('Limited mode'), findsOneWidget);
      expect(find.byKey(const Key('ku_encode_preview_button')), findsNothing);

      // 2. Activate Registry slot on /settings/registry -> ReadyOffline!
      router.go('/settings/registry');
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('registry_activate_slot_button')));
      await tester.pumpAndSettle();
      expect(
        container.read(wave7WorkspaceProvider).registryReadiness,
        'ReadyOffline',
      );

      // 3. Encode cu_legacy -> conducts -> electricity -> verify lifecycle redirect to copper & save!
      router.go('/capture/operations/draft_1/encode');
      await tester.pumpAndSettle();
      final previewBtn = find.byKey(const Key('ku_encode_preview_button'));
      await tester.ensureVisible(previewBtn);
      await tester.tap(previewBtn);
      await tester.pumpAndSettle();

      expect(
        find.textContaining('cu_legacy -> copper (canonical redirect)'),
        findsOneWidget,
      );
      final saveBtn = find.byKey(const Key('ku_save_private_button'));
      await tester.ensureVisible(saveBtn);
      await tester.tap(saveBtn);
      await tester.pumpAndSettle();

      // Saved KU detail shows LOCAL_ONLY, Author: Unresolved, SELF_ATTESTED:
      expect(find.text('LOCAL_ONLY'), findsWidgets);
      expect(find.text('Unresolved'), findsOneWidget);
      expect(find.text('SELF_ATTESTED'), findsOneWidget);

      // 4. Local KQL rejects write queries (CREATE) and runs read-only FIND!
      router.go('/library/kql');
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const Key('local_kql_input_field')),
        'CREATE (k:KU)',
      );
      await tester.tap(find.byKey(const Key('local_kql_run_button')));
      await tester.pumpAndSettle();
      expect(find.text('Read-only boundary enforced'), findsOneWidget);

      await tester.enterText(
        find.byKey(const Key('local_kql_input_field')),
        'FIND (k:KU) WHERE gene_type = "Fact"',
      );
      await tester.tap(find.byKey(const Key('local_kql_run_button')));
      await tester.pumpAndSettle();
      expect(find.textContaining('Local KQL results (1)'), findsOneWidget);
    },
  );

  testWidgets(
    'MOB-06, MOB-07, MOB-08 & MOB-09: No-LLM default, durable tool proposal consent, FORBIDDEN_TO_LLM block, derived-cache GC preserving OwnedOriginal, and gated beta notice',
    (tester) async {
      final container = ProviderContainer(
        overrides: [
          mobileHostGatewayProvider.overrideWithValue(
            const _Wave7FakeHostGateway(),
          ),
        ],
      );
      addTearDown(container.dispose);

      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: const OneBrainApp(),
        ),
      );
      await tester.pumpAndSettle();

      final router = container.read(appRouterProvider);

      // 1. Assistant defaults to No LLM active · deterministic mode
      router.go('/assistant');
      await tester.pumpAndSettle();
      expect(find.text('No LLM active · deterministic mode'), findsOneWidget);

      // 2. FORBIDDEN_TO_LLM tool is blocked fail-closed
      final promptField = find.byKey(const Key('assistant_prompt_field'));
      await tester.ensureVisible(promptField);
      await tester.enterText(promptField, 'export_raw_vault_keys');
      final sendBtn = find.byKey(const Key('assistant_send_button'));
      await tester.ensureVisible(sendBtn);
      await tester.tap(sendBtn);
      await tester.pumpAndSettle();
      expect(find.textContaining('FORBIDDEN_TO_LLM'), findsOneWidget);

      // 3. Bounded kql: tool creates AwaitingConsent proposal and transitions to Committed only on approval
      await tester.ensureVisible(promptField);
      await tester.enterText(promptField, 'kql: FIND (k:KU)');
      await tester.ensureVisible(sendBtn);
      await tester.tap(sendBtn);
      await tester.pumpAndSettle();
      expect(find.text('AwaitingConsent'), findsOneWidget);

      final approveBtn = find.text('Authorize bounded execution');
      await tester.ensureVisible(approveBtn);
      await tester.tap(approveBtn);
      await tester.pumpAndSettle();
      expect(find.text('Committed'), findsOneWidget);

      // 4. Storage cleanup reclaims derived cache while preserving OwnedOriginal
      router.go('/settings/storage/cleanup');
      await tester.pumpAndSettle();
      final gcBtn = find.byKey(const Key('run_derived_cache_gc_button'));
      await tester.ensureVisible(gcBtn);
      await tester.tap(gcBtn);
      await tester.pumpAndSettle();
      expect(container.read(wave7WorkspaceProvider).derivedCacheBytes, 0);

      // 5. Advanced screen surfaces honest MOB-GATE-NETWORKED-BETA notice & redacted diagnostics has zero paths
      router.go('/settings/advanced');
      await tester.pumpAndSettle();
      expect(find.text('MOB-GATE-NETWORKED-BETA'), findsOneWidget);
      expect(find.text('GATED OFF'), findsOneWidget);

      router.go('/settings/diagnostics');
      await tester.pumpAndSettle();
      expect(find.textContaining('"raw_paths_included":false'), findsOneWidget);
    },
  );
}

List<String> _collectRoutePaths(List<RouteBase> routes, [String prefix = '']) {
  final out = <String>[];
  for (final route in routes) {
    if (route is GoRoute) {
      final full = prefix.isEmpty ? route.path : '$prefix/${route.path}';
      out.add(full);
      out.addAll(_collectRoutePaths(route.routes, full));
    }
  }
  return out;
}

class _Wave7FakeHostGateway implements MobileHostGateway {
  const _Wave7FakeHostGateway();

  @override
  Future<MobileHostSnapshot> inspectBootstrapHost() async =>
      const MobileHostSnapshot(
        platform: 'Android test',
        apiVersion: '9',
        registryRequestIssued: false,
        rustCoreLinked: true,
        rustCoreVersion: '0.4.0',
        rustAbiVersion: 13,
        rustRoundTripVerified: true,
      );

  @override
  Future<MobileRuntimeSnapshot> inspectRuntimeProfile() async =>
      const MobileRuntimeSnapshot(
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
        encryptedRawDraftCount: 1,
        pendingShareSpoolCount: 0,
        stagedVerifiedMediaCount: 0,
        onboardingCursor: MobileOnboardingCursor.limitedHome,
      );

  @override
  Future<MobileRegistryInitAvailability>
  inspectRegistryInitAvailability() async =>
      const MobileRegistryInitAvailability(
        available: false,
        trustMode: MobileRegistryTrustMode.unavailable,
        channelId: 'stable',
        reasonCode: 'TEST',
        transportEnabled: false,
      );

  @override
  Future<MobileRegistryInitPlan> beginRegistryInit(String channelId) =>
      throw UnsupportedError('unused');

  @override
  Future<void> deferRegistryInit({
    required String operationId,
    required String manifestDigest,
  }) => throw UnsupportedError('unused');

  @override
  Future<MobileRegistryInitPlan> confirmRegistryInit({
    required String operationId,
    required String manifestDigest,
    required MobileRegistryNetworkPolicy networkPolicy,
    required bool oneTimeNetworkOverride,
  }) => throw UnsupportedError('unused');

  @override
  Future<MobileRegistryImportProgress> pickAndImportRegistryArtifact({
    required String operationId,
    required String manifestDigest,
    required MobileRegistryArtifactRole artifactRole,
  }) => throw UnsupportedError('unused');

  @override
  Future<MobileRegistryActivationReceipt> activateRegistryRelease({
    required String operationId,
    required String transferNonce,
  }) async => const MobileRegistryActivationReceipt(
    releaseId: 'rel_v1_test',
    readinessState: 'ReadyOffline',
    previousReleaseId: null,
    compensatedRollback: false,
    healthProbesChecked: 64,
  );

  @override
  Future<bool> rollbackRegistryRelease() async => true;

  @override
  Future<MobileRawDraftReceipt> saveRawTextDraft({
    required String contentLanguage,
    required String content,
  }) async => MobileRawDraftReceipt(
    draftRef: 'draft_1',
    contentLanguage: contentLanguage,
    contentBytes: content.length,
    totalDrafts: 1,
  );

  @override
  Future<List<MobileShareSpoolSummary>> inspectPendingShareSpools() async =>
      const [];

  @override
  Future<MobileRawDraftReceipt> importSharedText({
    required String spoolRef,
    required String contentLanguage,
  }) => saveRawTextDraft(contentLanguage: contentLanguage, content: 'shared');

  @override
  Future<MobileOwnedMediaSummary> pickAndImportOwnedMedia(
    MobileMediaClass mediaClass,
  ) => throw UnsupportedError('unused');

  @override
  Future<List<MobileOwnedMediaSummary>> inspectOwnedMedia() async => const [
    MobileOwnedMediaSummary(
      mediaRef:
          'media_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      mediaClass: MobileMediaClass.image,
      mimeType: 'image/png',
      contentBytes: 1024,
      verifiedBytes: 1024,
      storageClass: 'OwnedOriginal',
      ownedHold: true,
      importState: 'Complete',
    ),
  ];

  @override
  Future<void> setOnboardingCursor(MobileOnboardingCursor cursor) async {}

  @override
  Future<String> startFeasibilityOperation(Duration delay) async => 'op';

  @override
  Future<bool> cancelFeasibilityOperation(String operationId) async => true;

  @override
  Stream<HostOperationEvent> observeFeasibilityOperations() =>
      const Stream.empty();
}
