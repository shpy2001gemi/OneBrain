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
  String startFeasibilityOperation(int delayMilliseconds);

  @async
  bool cancelFeasibilityOperation(String operationId);
}

@EventChannelApi()
abstract class MobileHostEventApi {
  HostOperationEvent hostOperationEvents();
}
