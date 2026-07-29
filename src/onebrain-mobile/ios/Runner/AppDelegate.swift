import Flutter
import UIKit

private let hostApiVersion = "2"
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
  private let dataRoot: String?
  private let runtimeQueue = DispatchQueue(
    label: "org.onebrain.mobile.runtime",
    qos: .userInitiated
  )
  private var pending: [String: DispatchWorkItem] = [:]

  init(events: MobileHostEventStream, dataRoot: String?) {
    self.events = events
    self.dataRoot = dataRoot
  }

  func inspectRuntimeProfile(
    completion: @escaping (Result<HostRuntimeSnapshot, Error>) -> Void
  ) {
    guard let dataRoot else {
      completion(
        .failure(
          PigeonError(
            code: "RUNTIME_PATH_UNAVAILABLE",
            message: "Application Support storage is unavailable",
            details: nil
          )
        )
      )
      return
    }
    runtimeQueue.async {
      let pathBytes = Array(dataRoot.utf8)
      let runtime = pathBytes.withUnsafeBufferPointer { bytes in
        ob_mobile_runtime_open_utf8(bytes.baseAddress, bytes.count)
      }
      guard runtime.status_code == 0 else {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "RUNTIME_OPEN_FAILED",
                message: "Rust mobile runtime status \(runtime.status_code)",
                details: nil
              )
            )
          )
        }
        return
      }
      let activationPhase: String
      switch runtime.activation_phase {
      case 0:
        activationPhase = "Dormant"
      case 1:
        activationPhase = "Starting"
      case 2:
        activationPhase = "Active"
      case 3:
        activationPhase = "Draining"
      default:
        activationPhase = "Unknown"
      }
      let snapshot = HostRuntimeSnapshot(
        profileVersion: "MOB-02/1",
        processGeneration: Int64(runtime.process_generation),
        activationPhase: activationPhase,
        activeGrantCount: Int64(runtime.active_grant_count),
        recoveredUncleanStart: runtime.recovered_unclean_start != 0,
        bootstrapStoreOpened: runtime.bootstrap_store_opened != 0,
        registryState:
          runtime.registry_bootstrap_only != 0 ? "BootstrapOnly" : "Unknown",
        localKqlFixtureVerified: runtime.local_kql_fixture_verified != 0,
        privatePlannerVerified: runtime.private_planner_verified != 0,
        noLlmProvider: runtime.no_llm_provider != 0,
        staleCallbackRejected: runtime.stale_callback_rejected != 0
      )
      DispatchQueue.main.async {
        completion(.success(snapshot))
      }
    }
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
    let host = IOSMobileHost(events: events, dataRoot: Self.prepareRuntimeDataRoot())
    HostOperationEventsStreamHandler.register(
      with: messenger,
      streamHandler: events
    )
    MobileHostApiSetup.setUp(binaryMessenger: messenger, api: host)
    mobileHostEvents = events
    mobileHost = host
  }

  private static func prepareRuntimeDataRoot() -> String? {
    guard
      let applicationSupport = try? FileManager.default.url(
        for: .applicationSupportDirectory,
        in: .userDomainMask,
        appropriateFor: nil,
        create: true
      )
    else {
      return nil
    }
    let root = applicationSupport.appendingPathComponent(
      "OneBrainMobile",
      isDirectory: true
    )
    do {
      try FileManager.default.createDirectory(
        at: root,
        withIntermediateDirectories: true
      )
      return root.path
    } catch {
      return nil
    }
  }
}
