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
    required this.stagedVerifiedMediaCount,
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
  final int stagedVerifiedMediaCount;
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

enum MobileMediaClass { image, video, audio, document }

enum MobileRegistryTrustMode { unavailable, developmentFixture, production }

enum MobileRegistryNetworkPolicy { wifiOnly, unmetered, anyNetwork }

class MobileRegistryInitAvailability {
  const MobileRegistryInitAvailability({
    required this.available,
    required this.trustMode,
    required this.channelId,
    required this.reasonCode,
    required this.transportEnabled,
  });

  final bool available;
  final MobileRegistryTrustMode trustMode;
  final String channelId;
  final String reasonCode;
  final bool transportEnabled;
}

class MobileRegistryInitPlan {
  const MobileRegistryInitPlan({
    required this.operationId,
    required this.stateCode,
    required this.channelId,
    required this.releaseId,
    required this.manifestDigest,
    required this.trustProfileDigest,
    required this.headGeneration,
    required this.releaseSequence,
    required this.publisherMinAdditionalFreeBytes,
    required this.artifactTotalBytes,
    required this.targetTotalAllocBytes,
    required this.transferInitialBytes,
    required this.verificationWorkspaceBytes,
    required this.catalogGrowthBytes,
    required this.safetyReserveBytes,
    required this.destinationTotalUsableBytes,
    required this.measuredFreeBytes,
    required this.initialRequiredFreeBytes,
    required this.admitted,
    required this.transportEnabled,
    required this.trustMode,
  });

  final String operationId;
  final int stateCode;
  final String channelId;
  final String releaseId;
  final String manifestDigest;
  final String trustProfileDigest;
  final int headGeneration;
  final int releaseSequence;
  final int publisherMinAdditionalFreeBytes;
  final int artifactTotalBytes;
  final int targetTotalAllocBytes;
  final int transferInitialBytes;
  final int verificationWorkspaceBytes;
  final int catalogGrowthBytes;
  final int safetyReserveBytes;
  final int destinationTotalUsableBytes;
  final int measuredFreeBytes;
  final int initialRequiredFreeBytes;
  final bool admitted;
  final bool transportEnabled;
  final MobileRegistryTrustMode trustMode;
}

class MobileOwnedMediaSummary {
  const MobileOwnedMediaSummary({
    required this.mediaRef,
    required this.mediaClass,
    required this.mimeType,
    required this.contentBytes,
    required this.verifiedBytes,
    required this.storageClass,
    required this.ownedHold,
    required this.importState,
  });

  final String mediaRef;
  final MobileMediaClass mediaClass;
  final String mimeType;
  final int contentBytes;
  final int verifiedBytes;
  final String storageClass;
  final bool ownedHold;
  final String importState;
}

abstract interface class MobileHostGateway {
  Future<MobileHostSnapshot> inspectBootstrapHost();

  Future<MobileRuntimeSnapshot> inspectRuntimeProfile();

  Future<MobileRegistryInitAvailability> inspectRegistryInitAvailability();

  Future<MobileRegistryInitPlan> beginRegistryInit(String channelId);

  Future<void> deferRegistryInit({
    required String operationId,
    required String manifestDigest,
  });

  Future<MobileRegistryInitPlan> confirmRegistryInit({
    required String operationId,
    required String manifestDigest,
    required MobileRegistryNetworkPolicy networkPolicy,
    required bool oneTimeNetworkOverride,
  });

  Future<MobileRawDraftReceipt> saveRawTextDraft({
    required String contentLanguage,
    required String content,
  });

  Future<List<MobileShareSpoolSummary>> inspectPendingShareSpools();

  Future<MobileRawDraftReceipt> importSharedText({
    required String spoolRef,
    required String contentLanguage,
  });

  Future<MobileOwnedMediaSummary> pickAndImportOwnedMedia(
    MobileMediaClass mediaClass,
  );

  Future<List<MobileOwnedMediaSummary>> inspectOwnedMedia();

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
      stagedVerifiedMediaCount: snapshot.stagedVerifiedMediaCount,
      onboardingCursor:
          MobileOnboardingCursor.values[snapshot.onboardingCursor.index],
    );
  }

  @override
  Future<MobileRegistryInitAvailability>
  inspectRegistryInitAvailability() async {
    final availability = await _api.inspectRegistryInitAvailability();
    return MobileRegistryInitAvailability(
      available: availability.available,
      trustMode: MobileRegistryTrustMode.values[availability.trustMode.index],
      channelId: availability.channelId,
      reasonCode: availability.reasonCode,
      transportEnabled: availability.transportEnabled,
    );
  }

  @override
  Future<MobileRegistryInitPlan> beginRegistryInit(String channelId) async =>
      _registryPlanFromHost(await _api.beginRegistryInit(channelId));

  @override
  Future<void> deferRegistryInit({
    required String operationId,
    required String manifestDigest,
  }) async {
    final saved = await _api.deferRegistryInit(operationId, manifestDigest);
    if (!saved) {
      throw StateError('Native host rejected the exact Init defer receipt');
    }
  }

  @override
  Future<MobileRegistryInitPlan> confirmRegistryInit({
    required String operationId,
    required String manifestDigest,
    required MobileRegistryNetworkPolicy networkPolicy,
    required bool oneTimeNetworkOverride,
  }) async => _registryPlanFromHost(
    await _api.confirmRegistryInit(
      operationId,
      manifestDigest,
      HostRegistryNetworkPolicy.values[networkPolicy.index],
      oneTimeNetworkOverride,
    ),
  );

  MobileRegistryInitPlan _registryPlanFromHost(HostRegistryInitPlan plan) =>
      MobileRegistryInitPlan(
        operationId: plan.operationId,
        stateCode: plan.stateCode,
        channelId: plan.channelId,
        releaseId: plan.releaseId,
        manifestDigest: plan.manifestDigest,
        trustProfileDigest: plan.trustProfileDigest,
        headGeneration: plan.headGeneration,
        releaseSequence: plan.releaseSequence,
        publisherMinAdditionalFreeBytes: plan.publisherMinAdditionalFreeBytes,
        artifactTotalBytes: plan.artifactTotalBytes,
        targetTotalAllocBytes: plan.targetTotalAllocBytes,
        transferInitialBytes: plan.transferInitialBytes,
        verificationWorkspaceBytes: plan.verificationWorkspaceBytes,
        catalogGrowthBytes: plan.catalogGrowthBytes,
        safetyReserveBytes: plan.safetyReserveBytes,
        destinationTotalUsableBytes: plan.destinationTotalUsableBytes,
        measuredFreeBytes: plan.measuredFreeBytes,
        initialRequiredFreeBytes: plan.initialRequiredFreeBytes,
        admitted: plan.admitted,
        transportEnabled: plan.transportEnabled,
        trustMode: MobileRegistryTrustMode.values[plan.trustMode.index],
      );

  @override
  Future<MobileOwnedMediaSummary> pickAndImportOwnedMedia(
    MobileMediaClass mediaClass,
  ) async {
    final receipt = await _api.pickAndImportOwnedMedia(
      HostMediaClass.values[mediaClass.index],
    );
    return _ownedMediaFromHost(receipt);
  }

  @override
  Future<List<MobileOwnedMediaSummary>> inspectOwnedMedia() async {
    final entries = await _api.inspectOwnedMedia();
    return entries.map(_ownedMediaFromHost).toList(growable: false);
  }

  MobileOwnedMediaSummary _ownedMediaFromHost(HostOwnedMediaSummary receipt) {
    return MobileOwnedMediaSummary(
      mediaRef: receipt.mediaRef,
      mediaClass: MobileMediaClass.values[receipt.mediaClass.index],
      mimeType: receipt.mimeType,
      contentBytes: receipt.contentBytes,
      verifiedBytes: receipt.verifiedBytes,
      storageClass: receipt.storageClass,
      ownedHold: receipt.ownedHold,
      importState: receipt.importState,
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

final registryInitAvailabilityProvider =
    FutureProvider<MobileRegistryInitAvailability>(
      (ref) => ref
          .watch(mobileHostGatewayProvider)
          .inspectRegistryInitAvailability(),
    );

final pendingShareSpoolsProvider =
    FutureProvider<List<MobileShareSpoolSummary>>(
      (ref) => ref.watch(mobileHostGatewayProvider).inspectPendingShareSpools(),
    );

final ownedMediaProvider = FutureProvider<List<MobileOwnedMediaSummary>>(
  (ref) => ref.watch(mobileHostGatewayProvider).inspectOwnedMedia(),
);
