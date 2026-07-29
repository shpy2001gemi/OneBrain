import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'generated/mobile_host_api.g.dart';

enum MobileOnboardingCursor {
  welcome('/onboarding/welcome'),
  preflight('/onboarding/preflight'),
  identity('/onboarding/identity'),
  security('/onboarding/security'),
  initHandoff('/onboarding/init-handoff'),
  limitedHome('/home');

  const MobileOnboardingCursor(this.location);

  final String location;
}

class MobileHostSnapshot {
  const MobileHostSnapshot({
    required this.platform,
    required this.apiVersion,
    required this.registryRequestIssued,
    required this.rustCoreLinked,
    required this.rustCoreVersion,
    required this.rustAbiVersion,
    required this.rustRoundTripVerified,
  });

  final String platform;
  final String apiVersion;
  final bool registryRequestIssued;
  final bool rustCoreLinked;
  final String rustCoreVersion;
  final int rustAbiVersion;
  final bool rustRoundTripVerified;
}

class MobileRuntimeSnapshot {
  const MobileRuntimeSnapshot({
    required this.profileVersion,
    required this.processGeneration,
    required this.activationPhase,
    required this.activeGrantCount,
    required this.recoveredUncleanStart,
    required this.bootstrapStoreOpened,
    required this.registryState,
    required this.localKqlFixtureVerified,
    required this.privatePlannerVerified,
    required this.noLlmProvider,
    required this.staleCallbackRejected,
    required this.secureProfileActive,
    required this.installationBindingVerified,
    required this.installationCreated,
    required this.securitySessionUnlocked,
    required this.privateVaultReady,
    required this.identityDomainsSeparated,
    required this.privacyDefaultsFailSafe,
    required this.redactedHistoryReady,
    required this.encryptedRawDraftCount,
    required this.pendingShareSpoolCount,
    required this.onboardingCursor,
  });

  final String profileVersion;
  final int processGeneration;
  final String activationPhase;
  final int activeGrantCount;
  final bool recoveredUncleanStart;
  final bool bootstrapStoreOpened;
  final String registryState;
  final bool localKqlFixtureVerified;
  final bool privatePlannerVerified;
  final bool noLlmProvider;
  final bool staleCallbackRejected;
  final bool secureProfileActive;
  final bool installationBindingVerified;
  final bool installationCreated;
  final bool securitySessionUnlocked;
  final bool privateVaultReady;
  final bool identityDomainsSeparated;
  final bool privacyDefaultsFailSafe;
  final bool redactedHistoryReady;
  final int encryptedRawDraftCount;
  final int pendingShareSpoolCount;
  final MobileOnboardingCursor onboardingCursor;
}

class MobileRawDraftReceipt {
  const MobileRawDraftReceipt({
    required this.draftRef,
    required this.contentLanguage,
    required this.contentBytes,
    required this.totalDrafts,
  });

  final String draftRef;
  final String contentLanguage;
  final int contentBytes;
  final int totalDrafts;
}

class MobileShareSpoolSummary {
  const MobileShareSpoolSummary({
    required this.spoolRef,
    required this.mimeType,
    required this.contentBytes,
    required this.receivedAtMonotonicMillis,
  });

  final String spoolRef;
  final String mimeType;
  final int contentBytes;
  final int receivedAtMonotonicMillis;
}

abstract interface class MobileHostGateway {
  Future<MobileHostSnapshot> inspectBootstrapHost();

  Future<MobileRuntimeSnapshot> inspectRuntimeProfile();

  Future<MobileRawDraftReceipt> saveRawTextDraft({
    required String contentLanguage,
    required String content,
  });

  Future<List<MobileShareSpoolSummary>> inspectPendingShareSpools();

  Future<MobileRawDraftReceipt> importSharedText({
    required String spoolRef,
    required String contentLanguage,
  });

  Future<void> setOnboardingCursor(MobileOnboardingCursor cursor);

  Future<String> startFeasibilityOperation(Duration delay);

  Future<bool> cancelFeasibilityOperation(String operationId);

  Stream<HostOperationEvent> observeFeasibilityOperations();
}

class PigeonMobileHostGateway implements MobileHostGateway {
  PigeonMobileHostGateway({MobileHostApi? api})
    : _api = api ?? MobileHostApi(),
      _events = hostOperationEvents();

  final MobileHostApi _api;
  final Stream<HostOperationEvent> _events;

  @override
  Future<MobileHostSnapshot> inspectBootstrapHost() async {
    final snapshot = await _api.inspectBootstrapHost();
    return MobileHostSnapshot(
      platform: snapshot.platform,
      apiVersion: snapshot.apiVersion,
      registryRequestIssued: snapshot.registryRequestIssued,
      rustCoreLinked: snapshot.rustCoreLinked,
      rustCoreVersion: snapshot.rustCoreVersion,
      rustAbiVersion: snapshot.rustAbiVersion,
      rustRoundTripVerified: snapshot.rustRoundTripVerified,
    );
  }

  @override
  Future<MobileRuntimeSnapshot> inspectRuntimeProfile() async {
    final snapshot = await _api.inspectRuntimeProfile();
    return MobileRuntimeSnapshot(
      profileVersion: snapshot.profileVersion,
      processGeneration: snapshot.processGeneration,
      activationPhase: snapshot.activationPhase,
      activeGrantCount: snapshot.activeGrantCount,
      recoveredUncleanStart: snapshot.recoveredUncleanStart,
      bootstrapStoreOpened: snapshot.bootstrapStoreOpened,
      registryState: snapshot.registryState,
      localKqlFixtureVerified: snapshot.localKqlFixtureVerified,
      privatePlannerVerified: snapshot.privatePlannerVerified,
      noLlmProvider: snapshot.noLlmProvider,
      staleCallbackRejected: snapshot.staleCallbackRejected,
      secureProfileActive: snapshot.secureProfileActive,
      installationBindingVerified: snapshot.installationBindingVerified,
      installationCreated: snapshot.installationCreated,
      securitySessionUnlocked: snapshot.securitySessionUnlocked,
      privateVaultReady: snapshot.privateVaultReady,
      identityDomainsSeparated: snapshot.identityDomainsSeparated,
      privacyDefaultsFailSafe: snapshot.privacyDefaultsFailSafe,
      redactedHistoryReady: snapshot.redactedHistoryReady,
      encryptedRawDraftCount: snapshot.encryptedRawDraftCount,
      pendingShareSpoolCount: snapshot.pendingShareSpoolCount,
      onboardingCursor:
          MobileOnboardingCursor.values[snapshot.onboardingCursor.index],
    );
  }

  @override
  Future<List<MobileShareSpoolSummary>> inspectPendingShareSpools() async {
    final spools = await _api.inspectPendingShareSpools();
    return spools
        .map(
          (spool) => MobileShareSpoolSummary(
            spoolRef: spool.spoolRef,
            mimeType: spool.mimeType,
            contentBytes: spool.contentBytes,
            receivedAtMonotonicMillis: spool.receivedAtMonotonicMillis,
          ),
        )
        .toList(growable: false);
  }

  @override
  Future<MobileRawDraftReceipt> importSharedText({
    required String spoolRef,
    required String contentLanguage,
  }) async {
    final receipt = await _api.importSharedText(spoolRef, contentLanguage);
    return MobileRawDraftReceipt(
      draftRef: receipt.draftRef,
      contentLanguage: receipt.contentLanguage,
      contentBytes: receipt.contentBytes,
      totalDrafts: receipt.totalDrafts,
    );
  }

  @override
  Future<void> setOnboardingCursor(MobileOnboardingCursor cursor) async {
    final saved = await _api.setOnboardingCursor(
      HostOnboardingCursor.values[cursor.index],
    );
    if (!saved) {
      throw StateError('Native host rejected the onboarding cursor');
    }
  }

  @override
  Future<MobileRawDraftReceipt> saveRawTextDraft({
    required String contentLanguage,
    required String content,
  }) async {
    final receipt = await _api.saveRawTextDraft(contentLanguage, content);
    return MobileRawDraftReceipt(
      draftRef: receipt.draftRef,
      contentLanguage: receipt.contentLanguage,
      contentBytes: receipt.contentBytes,
      totalDrafts: receipt.totalDrafts,
    );
  }

  @override
  Future<String> startFeasibilityOperation(Duration delay) =>
      _api.startFeasibilityOperation(delay.inMilliseconds);

  @override
  Future<bool> cancelFeasibilityOperation(String operationId) =>
      _api.cancelFeasibilityOperation(operationId);

  @override
  Stream<HostOperationEvent> observeFeasibilityOperations() => _events;
}

final mobileHostGatewayProvider = Provider<MobileHostGateway>(
  (ref) => PigeonMobileHostGateway(),
);

final bootstrapHostSnapshotProvider = FutureProvider<MobileHostSnapshot>(
  (ref) => ref.watch(mobileHostGatewayProvider).inspectBootstrapHost(),
);

final mobileRuntimeSnapshotProvider = FutureProvider<MobileRuntimeSnapshot>(
  (ref) => ref.watch(mobileHostGatewayProvider).inspectRuntimeProfile(),
);

final pendingShareSpoolsProvider =
    FutureProvider<List<MobileShareSpoolSummary>>(
      (ref) => ref.watch(mobileHostGatewayProvider).inspectPendingShareSpools(),
    );
