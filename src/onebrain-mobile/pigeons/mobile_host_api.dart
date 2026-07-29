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
}

enum HostOperationEventKind { started, cancelled, completed }

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
  String startFeasibilityOperation(int delayMilliseconds);

  @async
  bool cancelFeasibilityOperation(String operationId);
}

@EventChannelApi()
abstract class MobileHostEventApi {
  HostOperationEvent hostOperationEvents();
}
