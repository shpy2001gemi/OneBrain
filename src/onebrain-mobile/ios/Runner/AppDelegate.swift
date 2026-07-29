import CryptoKit
import Flutter
import Security
import UIKit

private let hostApiVersion = "6"
private let maxFeasibilityDelayMilliseconds: Int64 = 30_000
private let rustRoundTripNonce: UInt64 = 0x4F_42_4D_30_31
private let securityMaterialBytes = 192
private let securityKeychainService = "org.onebrain.mobile.install-material.v1"
private let securityKeychainAccount = "current-installation"
private let securityMarkerMagic = Data("OBMARK01".utf8)
private let securityMarkerContext = Data("onebrain:mobile:install-marker:1\0".utf8)

private enum IOSSecurityMaterialError: Error {
  case unexpectedRestore(String)
  case protectedDataUnavailable
  case storage(String)
}

private final class IOSSecurityMaterialStore {
  private let dataRoot: URL
  private let marker: URL

  init(dataRoot: URL) {
    self.dataRoot = dataRoot
    marker = dataRoot
      .appendingPathComponent("security", isDirectory: true)
      .appendingPathComponent("install-marker.v1", isDirectory: false)
  }

  func loadOrCreate() throws -> Data {
    let markerExists = FileManager.default.fileExists(atPath: marker.path)
    let keychainMaterial = try readKeychainMaterial()
    if !markerExists {
      if FileManager.default.fileExists(
        atPath: dataRoot.appendingPathComponent("bootstrap.redb").path
      )
        || FileManager.default.fileExists(
          atPath: dataRoot.appendingPathComponent("private-vault.redb").path
        )
        || FileManager.default.fileExists(
          atPath: dataRoot.appendingPathComponent("private-drafts.redb").path
        )
      {
        throw IOSSecurityMaterialError.unexpectedRestore(
          "authority bytes exist without the excluded install marker"
        )
      }
      if keychainMaterial != nil {
        try deleteKeychainMaterial()
      }
      return try createInstallation()
    }
    guard let material = keychainMaterial else {
      throw IOSSecurityMaterialError.unexpectedRestore(
        "install marker exists without its this-device-only protected item"
      )
    }
    guard material.count == securityMaterialBytes else {
      throw IOSSecurityMaterialError.unexpectedRestore(
        "protected item has an invalid length"
      )
    }
    let storedMarker = try Data(contentsOf: marker)
    guard storedMarker == markerBytes(material) else {
      throw IOSSecurityMaterialError.unexpectedRestore(
        "install marker does not bind the protected item"
      )
    }
    return material
  }

  private func createInstallation() throws -> Data {
    let securityDirectory = marker.deletingLastPathComponent()
    try FileManager.default.createDirectory(
      at: securityDirectory,
      withIntermediateDirectories: true
    )
    var material = Data(count: securityMaterialBytes)
    let randomStatus = material.withUnsafeMutableBytes { bytes in
      SecRandomCopyBytes(kSecRandomDefault, securityMaterialBytes, bytes.baseAddress!)
    }
    guard randomStatus == errSecSuccess else {
      material.resetBytes(in: 0..<material.count)
      throw IOSSecurityMaterialError.storage(
        "SecRandomCopyBytes failed with status \(randomStatus)"
      )
    }
    do {
      try writeKeychainMaterial(material)
      try markerBytes(material).write(to: marker, options: [.atomic, .completeFileProtection])
      return material
    } catch {
      material.resetBytes(in: 0..<material.count)
      try? deleteKeychainMaterial()
      try? FileManager.default.removeItem(at: marker)
      throw error
    }
  }

  private func readKeychainMaterial() throws -> Data? {
    let query: [CFString: Any] = [
      kSecClass: kSecClassGenericPassword,
      kSecAttrService: securityKeychainService,
      kSecAttrAccount: securityKeychainAccount,
      kSecReturnData: true,
      kSecMatchLimit: kSecMatchLimitOne,
    ]
    var result: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &result)
    switch status {
    case errSecSuccess:
      return result as? Data
    case errSecItemNotFound:
      return nil
    case errSecInteractionNotAllowed:
      throw IOSSecurityMaterialError.protectedDataUnavailable
    default:
      throw IOSSecurityMaterialError.storage(
        "Keychain read failed with status \(status)"
      )
    }
  }

  private func writeKeychainMaterial(_ material: Data) throws {
    let attributes: [CFString: Any] = [
      kSecClass: kSecClassGenericPassword,
      kSecAttrService: securityKeychainService,
      kSecAttrAccount: securityKeychainAccount,
      kSecAttrAccessible: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
      kSecValueData: material,
    ]
    let status = SecItemAdd(attributes as CFDictionary, nil)
    guard status == errSecSuccess else {
      throw IOSSecurityMaterialError.storage(
        "Keychain create failed with status \(status)"
      )
    }
  }

  private func deleteKeychainMaterial() throws {
    let query: [CFString: Any] = [
      kSecClass: kSecClassGenericPassword,
      kSecAttrService: securityKeychainService,
      kSecAttrAccount: securityKeychainAccount,
    ]
    let status = SecItemDelete(query as CFDictionary)
    guard status == errSecSuccess || status == errSecItemNotFound else {
      throw IOSSecurityMaterialError.storage(
        "Keychain retirement failed with status \(status)"
      )
    }
  }

  private func markerBytes(_ material: Data) -> Data {
    var input = Data()
    input.append(securityMarkerContext)
    input.append(material)
    var marker = securityMarkerMagic
    marker.append(contentsOf: SHA256.hash(data: input))
    return marker
  }
}

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
  private let securityMaterialStore: IOSSecurityMaterialStore?
  private let runtimeQueue = DispatchQueue(
    label: "org.onebrain.mobile.runtime",
    qos: .userInitiated
  )
  private var pending: [String: DispatchWorkItem] = [:]

  init(
    events: MobileHostEventStream,
    dataRoot: String?,
    securityMaterialStore: IOSSecurityMaterialStore?
  ) {
    self.events = events
    self.dataRoot = dataRoot
    self.securityMaterialStore = securityMaterialStore
  }

  func inspectRuntimeProfile(
    completion: @escaping (Result<HostRuntimeSnapshot, Error>) -> Void
  ) {
    guard let dataRoot, let securityMaterialStore else {
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
      var securityMaterial: Data
      do {
        securityMaterial = try securityMaterialStore.loadOrCreate()
      } catch {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "SECURE_MATERIAL_UNAVAILABLE",
                message: "Protected installation material is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      defer {
        securityMaterial.resetBytes(in: 0..<securityMaterial.count)
      }
      let pathBytes = Array(dataRoot.utf8)
      let runtime = pathBytes.withUnsafeBufferPointer { bytes in
        securityMaterial.withUnsafeBytes { material in
          ob_mobile_runtime_open_secure_utf8(
            bytes.baseAddress,
            bytes.count,
            material.bindMemory(to: UInt8.self).baseAddress,
            material.count
          )
        }
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
        profileVersion: "MOB-04/2",
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
        staleCallbackRejected: runtime.stale_callback_rejected != 0,
        secureProfileActive: runtime.secure_profile_active != 0,
        installationBindingVerified: runtime.installation_binding_verified != 0,
        installationCreated: runtime.installation_created != 0,
        securitySessionUnlocked: runtime.security_session_unlocked != 0,
        privateVaultReady: runtime.private_vault_ready != 0,
        identityDomainsSeparated: runtime.identity_domains_separated != 0,
        privacyDefaultsFailSafe: runtime.privacy_defaults_fail_safe != 0,
        redactedHistoryReady: runtime.redacted_history_ready != 0,
        encryptedRawDraftCount: Int64(runtime.encrypted_raw_draft_count),
        pendingShareSpoolCount: Int64(runtime.pending_share_spool_count),
        onboardingCursor:
          HostOnboardingCursor(rawValue: Int(runtime.onboarding_cursor)) ?? .welcome
      )
      DispatchQueue.main.async {
        completion(.success(snapshot))
      }
    }
  }

  func saveRawTextDraft(
    contentLanguage: String,
    content: String,
    completion: @escaping (Result<HostRawDraftReceipt, Error>) -> Void
  ) {
    guard let dataRoot, let securityMaterialStore else {
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
      var securityMaterial: Data
      do {
        securityMaterial = try securityMaterialStore.loadOrCreate()
      } catch {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "SECURE_MATERIAL_UNAVAILABLE",
                message: "Protected installation material is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      defer {
        securityMaterial.resetBytes(in: 0..<securityMaterial.count)
      }
      let pathBytes = Array(dataRoot.utf8)
      let runtime = pathBytes.withUnsafeBufferPointer { bytes in
        securityMaterial.withUnsafeBytes { material in
          ob_mobile_runtime_open_secure_utf8(
            bytes.baseAddress,
            bytes.count,
            material.bindMemory(to: UInt8.self).baseAddress,
            material.count
          )
        }
      }
      guard runtime.status_code == 0 else {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "RUNTIME_OPEN_FAILED",
                message: "Protected runtime session is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      let languageBytes = Array(contentLanguage.utf8)
      let contentBytes = Array(content.utf8)
      let receipt = languageBytes.withUnsafeBufferPointer { language in
        contentBytes.withUnsafeBufferPointer { text in
          ob_mobile_runtime_save_raw_text_draft_utf8(
            language.baseAddress,
            language.count,
            text.baseAddress,
            text.count
          )
        }
      }
      guard receipt.status_code == 0 else {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "RAW_DRAFT_SAVE_FAILED",
                message: "Private draft could not be saved",
                details: nil
              )
            )
          )
        }
        return
      }
      let draftRef = withUnsafeBytes(of: receipt.draft_ref) { bytes in
        String(
          decoding: bytes.prefix(Int(receipt.draft_ref_len)),
          as: UTF8.self
        )
      }
      let language = withUnsafeBytes(of: receipt.content_language) { bytes in
        String(
          decoding: bytes.prefix(Int(receipt.content_language_len)),
          as: UTF8.self
        )
      }
      let result = HostRawDraftReceipt(
        draftRef: draftRef,
        contentLanguage: language,
        contentBytes: Int64(receipt.content_bytes),
        totalDrafts: Int64(receipt.total_drafts)
      )
      DispatchQueue.main.async {
        completion(.success(result))
      }
    }
  }

  func inspectPendingShareSpools(
    completion: @escaping (Result<[HostShareSpoolSummary], Error>) -> Void
  ) {
    guard let dataRoot, let securityMaterialStore else {
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
      var securityMaterial: Data
      do {
        securityMaterial = try securityMaterialStore.loadOrCreate()
      } catch {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "SECURE_MATERIAL_UNAVAILABLE",
                message: "Protected installation material is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      defer {
        securityMaterial.resetBytes(in: 0..<securityMaterial.count)
      }
      let pathBytes = Array(dataRoot.utf8)
      let runtime = pathBytes.withUnsafeBufferPointer { bytes in
        securityMaterial.withUnsafeBytes { material in
          ob_mobile_runtime_open_secure_utf8(
            bytes.baseAddress,
            bytes.count,
            material.bindMemory(to: UInt8.self).baseAddress,
            material.count
          )
        }
      }
      guard runtime.status_code == 0 else {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "RUNTIME_OPEN_FAILED",
                message: "Protected runtime session is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      let count = min(Int(runtime.pending_share_spool_count), 64)
      var summaries: [HostShareSpoolSummary] = []
      summaries.reserveCapacity(count)
      for index in 0..<count {
        let summary = ob_mobile_runtime_pending_share_spool_at(index)
        guard summary.status_code == 0 else {
          DispatchQueue.main.async {
            completion(
              .failure(
                PigeonError(
                  code: "SHARE_SPOOL_INSPECT_FAILED",
                  message: "Private share spool could not be inspected",
                  details: nil
                )
              )
            )
          }
          return
        }
        let spoolRef = withUnsafeBytes(of: summary.spool_ref) { bytes in
          String(
            decoding: bytes.prefix(Int(summary.spool_ref_len)),
            as: UTF8.self
          )
        }
        let mimeType = withUnsafeBytes(of: summary.mime_type) { bytes in
          String(
            decoding: bytes.prefix(Int(summary.mime_type_len)),
            as: UTF8.self
          )
        }
        summaries.append(
          HostShareSpoolSummary(
            spoolRef: spoolRef,
            mimeType: mimeType,
            contentBytes: Int64(summary.content_bytes),
            receivedAtMonotonicMillis:
              Int64(summary.received_at_monotonic_ms)
          )
        )
      }
      DispatchQueue.main.async {
        completion(.success(summaries))
      }
    }
  }

  func importSharedText(
    spoolRef: String,
    contentLanguage: String,
    completion: @escaping (Result<HostRawDraftReceipt, Error>) -> Void
  ) {
    guard let dataRoot, let securityMaterialStore else {
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
      var securityMaterial: Data
      do {
        securityMaterial = try securityMaterialStore.loadOrCreate()
      } catch {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "SECURE_MATERIAL_UNAVAILABLE",
                message: "Protected installation material is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      defer {
        securityMaterial.resetBytes(in: 0..<securityMaterial.count)
      }
      let pathBytes = Array(dataRoot.utf8)
      let runtime = pathBytes.withUnsafeBufferPointer { bytes in
        securityMaterial.withUnsafeBytes { material in
          ob_mobile_runtime_open_secure_utf8(
            bytes.baseAddress,
            bytes.count,
            material.bindMemory(to: UInt8.self).baseAddress,
            material.count
          )
        }
      }
      guard runtime.status_code == 0 else {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "RUNTIME_OPEN_FAILED",
                message: "Protected runtime session is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      let spoolBytes = Array(spoolRef.utf8)
      let languageBytes = Array(contentLanguage.utf8)
      let receipt = spoolBytes.withUnsafeBufferPointer { spool in
        languageBytes.withUnsafeBufferPointer { language in
          ob_mobile_runtime_import_shared_text_utf8(
            spool.baseAddress,
            spool.count,
            language.baseAddress,
            language.count
          )
        }
      }
      guard receipt.status_code == 0 else {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "SHARE_SPOOL_IMPORT_FAILED",
                message: "Shared text could not be imported",
                details: nil
              )
            )
          )
        }
        return
      }
      let draftRef = withUnsafeBytes(of: receipt.draft_ref) { bytes in
        String(
          decoding: bytes.prefix(Int(receipt.draft_ref_len)),
          as: UTF8.self
        )
      }
      let language = withUnsafeBytes(of: receipt.content_language) { bytes in
        String(
          decoding: bytes.prefix(Int(receipt.content_language_len)),
          as: UTF8.self
        )
      }
      let result = HostRawDraftReceipt(
        draftRef: draftRef,
        contentLanguage: language,
        contentBytes: Int64(receipt.content_bytes),
        totalDrafts: Int64(receipt.total_drafts)
      )
      DispatchQueue.main.async {
        completion(.success(result))
      }
    }
  }

  func setOnboardingCursor(
    cursor: HostOnboardingCursor,
    completion: @escaping (Result<Bool, Error>) -> Void
  ) {
    guard let dataRoot, let securityMaterialStore else {
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
      var securityMaterial: Data
      do {
        securityMaterial = try securityMaterialStore.loadOrCreate()
      } catch {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "SECURE_MATERIAL_UNAVAILABLE",
                message: "Protected installation material is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      defer {
        securityMaterial.resetBytes(in: 0..<securityMaterial.count)
      }
      let pathBytes = Array(dataRoot.utf8)
      let runtime = pathBytes.withUnsafeBufferPointer { bytes in
        securityMaterial.withUnsafeBytes { material in
          ob_mobile_runtime_open_secure_utf8(
            bytes.baseAddress,
            bytes.count,
            material.bindMemory(to: UInt8.self).baseAddress,
            material.count
          )
        }
      }
      guard runtime.status_code == 0 else {
        DispatchQueue.main.async {
          completion(
            .failure(
              PigeonError(
                code: "RUNTIME_OPEN_FAILED",
                message: "Protected runtime session is unavailable",
                details: nil
              )
            )
          )
        }
        return
      }
      let saved =
        ob_mobile_runtime_set_onboarding_cursor(UInt32(cursor.rawValue)) == 0
      DispatchQueue.main.async {
        completion(.success(saved))
      }
    }
  }

  func lockPrivateNode() {
    runtimeQueue.async {
      _ = ob_mobile_runtime_lock_private_node()
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

  override func applicationDidEnterBackground(_ application: UIApplication) {
    mobileHost?.lockPrivateNode()
    super.applicationDidEnterBackground(application)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    let messenger = engineBridge.applicationRegistrar.messenger()
    let events = MobileHostEventStream()
    let root = Self.prepareRuntimeDataRoot()
    let host = IOSMobileHost(
      events: events,
      dataRoot: root?.path,
      securityMaterialStore: root.map { IOSSecurityMaterialStore(dataRoot: $0) }
    )
    HostOperationEventsStreamHandler.register(
      with: messenger,
      streamHandler: events
    )
    MobileHostApiSetup.setUp(binaryMessenger: messenger, api: host)
    mobileHostEvents = events
    mobileHost = host
  }

  private static func prepareRuntimeDataRoot() -> URL? {
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
      var excludedRoot = root
      var values = URLResourceValues()
      values.isExcludedFromBackup = true
      try excludedRoot.setResourceValues(values)
      return excludedRoot
    } catch {
      return nil
    }
  }
}
