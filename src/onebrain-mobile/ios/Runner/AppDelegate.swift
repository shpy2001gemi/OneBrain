import Flutter
import UIKit

private let hostApiVersion = "1"
private let maxFeasibilityDelayMilliseconds: Int64 = 30_000
private let rustRoundTripNonce: UInt64 = 0x4F_42_4D_30_31

private final class MobileHostEventStream: HostOperationEventsStreamHandler {
  private var sink: PigeonEventSink<HostOperationEvent>?

  override func onListen(
    withArguments arguments: Any?,
    sink: PigeonEventSink<HostOperationEvent>
  ) {
    self.sink = sink
  }

  override func onCancel(withArguments arguments: Any?) {
    sink = nil
  }

  func emit(_ event: HostOperationEvent) {
    sink?.success(event)
  }
}

private final class IOSMobileHost: MobileHostApi {
  private let events: MobileHostEventStream
  private var pending: [String: DispatchWorkItem] = [:]

  init(events: MobileHostEventStream) {
    self.events = events
  }

  func inspectBootstrapHost(
    completion: @escaping (Result<HostBootstrapSnapshot, Error>) -> Void
  ) {
    let coreVersion = String(cString: ob_mobile_bridge_core_version())
    completion(
      .success(
        HostBootstrapSnapshot(
          platform: "iOS \(UIDevice.current.systemVersion)",
          apiVersion: hostApiVersion,
          registryRequestIssued: ob_mobile_bridge_registry_request_issued() != 0,
          rustCoreLinked: true,
          rustCoreVersion: coreVersion,
          rustAbiVersion: Int64(ob_mobile_bridge_abi_version()),
          rustRoundTripVerified:
            ob_mobile_bridge_round_trip(rustRoundTripNonce) == rustRoundTripNonce
        )
      )
    )
  }

  func startFeasibilityOperation(
    delayMilliseconds: Int64,
    completion: @escaping (Result<String, Error>) -> Void
  ) {
    guard 0...maxFeasibilityDelayMilliseconds ~= delayMilliseconds else {
      completion(
        .failure(
          PigeonError(
            code: "HOST_INVALID_DELAY",
            message: "delayMilliseconds must be between 0 and 30000",
            details: nil
          )
        )
      )
      return
    }

    let operationId = UUID().uuidString
    let workItem = DispatchWorkItem { [weak self] in
      guard let self, pending.removeValue(forKey: operationId) != nil else {
        return
      }
      events.emit(
        HostOperationEvent(
          operationId: operationId,
          kind: .completed,
          code: "HOST_OPERATION_COMPLETED"
        )
      )
    }
    pending[operationId] = workItem
    events.emit(
      HostOperationEvent(
        operationId: operationId,
        kind: .started,
        code: "HOST_OPERATION_STARTED"
      )
    )
    DispatchQueue.main.asyncAfter(
      deadline: .now() + .milliseconds(Int(delayMilliseconds)),
      execute: workItem
    )
    completion(.success(operationId))
  }

  func cancelFeasibilityOperation(
    operationId: String,
    completion: @escaping (Result<Bool, Error>) -> Void
  ) {
    guard let workItem = pending.removeValue(forKey: operationId) else {
      completion(.success(false))
      return
    }
    workItem.cancel()
    events.emit(
      HostOperationEvent(
        operationId: operationId,
        kind: .cancelled,
        code: "HOST_OPERATION_CANCELLED"
      )
    )
    completion(.success(true))
  }
}

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  private var mobileHost: IOSMobileHost?
  private var mobileHostEvents: MobileHostEventStream?

  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    let messenger = engineBridge.applicationRegistrar.messenger()
    let events = MobileHostEventStream()
    let host = IOSMobileHost(events: events)
    HostOperationEventsStreamHandler.register(
      with: messenger,
      streamHandler: events
    )
    MobileHostApiSetup.setUp(binaryMessenger: messenger, api: host)
    mobileHostEvents = events
    mobileHost = host
  }
}
