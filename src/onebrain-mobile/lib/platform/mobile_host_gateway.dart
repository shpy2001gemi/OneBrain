import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'generated/mobile_host_api.g.dart';

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
}

abstract interface class MobileHostGateway {
  Future<MobileHostSnapshot> inspectBootstrapHost();

  Future<MobileRuntimeSnapshot> inspectRuntimeProfile();

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
