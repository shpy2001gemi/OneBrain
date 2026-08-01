import 'package:pigeon/pigeon.dart';

@ConfigurePigeon(
  PigeonOptions(
    dartOut: 'lib/platform/generated/mobile_host_api.g.dart',
    dartOptions: DartOptions(),
    dartPackageName: 'onebrain_mobile',
    kotlinOut:
        'android/app/src/main/kotlin/org/onebrain/onebrain_mobile/generated/MobileHostApi.g.kt',
    kotlinOptions: KotlinOptions(
      package: 'org.onebrain.onebrain_mobile.generated',
    ),
    swiftOut: 'ios/Runner/Generated/MobileHostApi.g.swift',
    swiftOptions: SwiftOptions(),
  ),
)
class HostBootstrapSnapshot {
  HostBootstrapSnapshot({
    required this.platform,
    required this.apiVersion,
    required this.registryRequestIssued,
    required this.rustCoreLinked,
    required this.rustCoreVersion,
    required this.rustAbiVersion,
    required this.rustRoundTripVerified,
  });

  String platform;
  String apiVersion;
  bool registryRequestIssued;
  bool rustCoreLinked;
  String rustCoreVersion;
  int rustAbiVersion;
  bool rustRoundTripVerified;
}

enum HostOnboardingCursor {
  welcome,
  preflight,
  identity,
  security,
  initHandoff,
  limitedHome,
}

class HostRuntimeSnapshot {
  HostRuntimeSnapshot({
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

  String profileVersion;
  int processGeneration;
  String activationPhase;
  int activeGrantCount;
  bool recoveredUncleanStart;
  bool bootstrapStoreOpened;
  String registryState;
  bool localKqlFixtureVerified;
  bool privatePlannerVerified;
  bool noLlmProvider;
  bool staleCallbackRejected;
  bool secureProfileActive;
  bool installationBindingVerified;
  bool installationCreated;
  bool securitySessionUnlocked;
  bool privateVaultReady;
  bool identityDomainsSeparated;
  bool privacyDefaultsFailSafe;
  bool redactedHistoryReady;
  int encryptedRawDraftCount;
  int pendingShareSpoolCount;
  int stagedVerifiedMediaCount;
  HostOnboardingCursor onboardingCursor;
}

class HostRawDraftReceipt {
  HostRawDraftReceipt({
    required this.draftRef,
    required this.contentLanguage,
    required this.contentBytes,
    required this.totalDrafts,
  });

  String draftRef;
  String contentLanguage;
  int contentBytes;
  int totalDrafts;
}

class HostShareSpoolSummary {
  HostShareSpoolSummary({
    required this.spoolRef,
    required this.mimeType,
    required this.contentBytes,
    required this.receivedAtMonotonicMillis,
  });

  String spoolRef;
  String mimeType;
  int contentBytes;
  int receivedAtMonotonicMillis;
}

enum HostMediaClass { image, video, audio, document }

class HostOwnedMediaSummary {
  HostOwnedMediaSummary({
    required this.mediaRef,
    required this.mediaClass,
    required this.mimeType,
    required this.contentBytes,
    required this.verifiedBytes,
    required this.storageClass,
    required this.ownedHold,
    required this.importState,
  });

  String mediaRef;
  HostMediaClass mediaClass;
  String mimeType;
  int contentBytes;
  int verifiedBytes;
  String storageClass;
  bool ownedHold;
  String importState;
}

enum HostOperationEventKind { started, cancelled, completed }

enum HostRegistryTrustMode { unavailable, developmentFixture, production }

enum HostRegistryNetworkPolicy { wifiOnly, unmetered, anyNetwork }

class HostRegistryInitAvailability {
  HostRegistryInitAvailability({
    required this.available,
    required this.trustMode,
    required this.channelId,
    required this.reasonCode,
    required this.transportEnabled,
  });

  bool available;
  HostRegistryTrustMode trustMode;
  String channelId;
  String reasonCode;
  bool transportEnabled;
}

class HostRegistryInitPlan {
  HostRegistryInitPlan({
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

  String operationId;
  int stateCode;
  String channelId;
  String releaseId;
  String manifestDigest;
  String trustProfileDigest;
  int headGeneration;
  int releaseSequence;
  int publisherMinAdditionalFreeBytes;
  int artifactTotalBytes;
  int targetTotalAllocBytes;
  int transferInitialBytes;
  int verificationWorkspaceBytes;
  int catalogGrowthBytes;
  int safetyReserveBytes;
  int destinationTotalUsableBytes;
  int measuredFreeBytes;
  int initialRequiredFreeBytes;
  bool admitted;
  bool transportEnabled;
  HostRegistryTrustMode trustMode;
}

class HostOperationEvent {
  HostOperationEvent({
    required this.operationId,
    required this.kind,
    required this.code,
  });

  String operationId;
  HostOperationEventKind kind;
  String code;
}

@HostApi()
abstract class MobileHostApi {
  @async
  HostBootstrapSnapshot inspectBootstrapHost();

  @async
  HostRuntimeSnapshot inspectRuntimeProfile();

  @async
  HostRegistryInitAvailability inspectRegistryInitAvailability();

  @async
  HostRegistryInitPlan beginRegistryInit(String channelId);

  @async
  bool deferRegistryInit(String operationId, String manifestDigest);

  @async
  HostRegistryInitPlan confirmRegistryInit(
    String operationId,
    String manifestDigest,
    HostRegistryNetworkPolicy networkPolicy,
    bool oneTimeNetworkOverride,
  );

  @async
  HostRawDraftReceipt saveRawTextDraft(String contentLanguage, String content);

  @async
  List<HostShareSpoolSummary> inspectPendingShareSpools();

  @async
  HostRawDraftReceipt importSharedText(String spoolRef, String contentLanguage);

  @async
  HostOwnedMediaSummary pickAndImportOwnedMedia(HostMediaClass mediaClass);

  @async
  List<HostOwnedMediaSummary> inspectOwnedMedia();

  @async
  bool setOnboardingCursor(HostOnboardingCursor cursor);

  @async
  String startFeasibilityOperation(int delayMilliseconds);

  @async
  bool cancelFeasibilityOperation(String operationId);
}

@EventChannelApi()
abstract class MobileHostEventApi {
  HostOperationEvent hostOperationEvents();
}
