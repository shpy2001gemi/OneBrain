import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'generated/mobile_host_api.g.dart';

class MobileHostSnapshot {
  const MobileHostSnapshot({
    required this.platform,
    required this.apiVersion,
    required this.registryRequestIssued,
    required this.rustCoreLinked,
  });

  final String platform;
  final String apiVersion;
  final bool registryRequestIssued;
  final bool rustCoreLinked;
}

abstract interface class MobileHostGateway {
  Future<MobileHostSnapshot> inspectBootstrapHost();

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
