package org.onebrain.onebrain_mobile

import android.content.Intent
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import java.util.UUID
import java.util.concurrent.Executors
import org.onebrain.onebrain_mobile.generated.FlutterError
import org.onebrain.onebrain_mobile.generated.HostBootstrapSnapshot
import org.onebrain.onebrain_mobile.generated.HostOperationEvent
import org.onebrain.onebrain_mobile.generated.HostOperationEventKind
import org.onebrain.onebrain_mobile.generated.HostOperationEventsStreamHandler
import org.onebrain.onebrain_mobile.generated.HostOnboardingCursor
import org.onebrain.onebrain_mobile.generated.HostRuntimeSnapshot
import org.onebrain.onebrain_mobile.generated.HostRawDraftReceipt
import org.onebrain.onebrain_mobile.generated.MobileHostApi
import org.onebrain.onebrain_mobile.generated.PigeonEventSink
import org.onebrain.onebrain_mobile.generated.HostShareSpoolSummary

private const val HOST_API_VERSION = "6"
private const val MAX_FEASIBILITY_DELAY_MILLIS = 30_000L
private const val RUNTIME_LOG_TAG = "OneBrainMobileRuntime"
private const val SHARE_CALLBACK_TOKEN =
    "org.onebrain.onebrain_mobile.extra.SHARE_CALLBACK_TOKEN"

class MainActivity : FlutterActivity() {
    private lateinit var hostApi: AndroidMobileHost
    private lateinit var hostEvents: AndroidHostEvents

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        hostEvents = AndroidHostEvents()
        hostApi =
            AndroidMobileHost(
                applicationContext.noBackupFilesDir.absolutePath,
                SecurityMaterialStore(applicationContext),
                hostEvents,
            )
        val messenger = flutterEngine.dartExecutor.binaryMessenger
        MobileHostApi.setUp(messenger, hostApi)
        HostOperationEventsStreamHandler.register(messenger, hostEvents)
        hostApi.acceptShareIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (::hostApi.isInitialized) {
            hostApi.acceptShareIntent(intent)
        }
    }

    override fun onStop() {
        hostApi.lockPrivateNode()
        super.onStop()
    }
}

private class AndroidHostEvents : HostOperationEventsStreamHandler() {
    private var sink: PigeonEventSink<HostOperationEvent>? = null

    override fun onListen(p0: Any?, sink: PigeonEventSink<HostOperationEvent>) {
        this.sink = sink
    }

    override fun onCancel(p0: Any?) {
        sink = null
    }

    fun emit(event: HostOperationEvent) {
        sink?.success(event)
    }
}

private class AndroidMobileHost(
    private val dataRoot: String,
    private val securityMaterialStore: SecurityMaterialStore,
    private val events: AndroidHostEvents,
) : MobileHostApi {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val runtimeExecutor =
        Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "onebrain-mobile-runtime")
        }
    private val pending = mutableMapOf<String, Runnable>()

    fun acceptShareIntent(intent: Intent?) {
        if (intent?.action != Intent.ACTION_SEND) {
            return
        }
        val mimeType = intent.type?.lowercase() ?: return
        val content = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString() ?: return
        val callbackToken =
            intent.getStringExtra(SHARE_CALLBACK_TOKEN)
                ?: "android:${UUID.randomUUID()}".also {
                    intent.putExtra(SHARE_CALLBACK_TOKEN, it)
                }
        runtimeExecutor.execute {
            runCatching {
                val securityMaterial = securityMaterialStore.loadOrCreate()
                try {
                    RustMobileBridge.enqueueSharedText(
                        dataRoot,
                        securityMaterial,
                        callbackToken,
                        mimeType,
                        content,
                    )
                } finally {
                    securityMaterial.fill(0)
                }
            }.onSuccess { spoolRef ->
                Log.i(
                    RUNTIME_LOG_TAG,
                    "share_spool_landed ref=${spoolRef.take(14)} type=$mimeType",
                )
            }.onFailure {
                Log.w(
                    RUNTIME_LOG_TAG,
                    "share_spool_rejected",
                )
            }
        }
    }

    override fun inspectBootstrapHost(
        callback: (Result<HostBootstrapSnapshot>) -> Unit,
    ) {
        val rust = RustMobileBridge.inspectBootstrap()
        callback(
            Result.success(
                HostBootstrapSnapshot(
                    platform = "Android ${Build.VERSION.RELEASE}",
                    apiVersion = HOST_API_VERSION,
                    registryRequestIssued = rust.registryRequestIssued,
                    rustCoreLinked = rust.linked,
                    rustCoreVersion = rust.coreVersion,
                    rustAbiVersion = rust.abiVersion,
                    rustRoundTripVerified = rust.roundTripVerified,
                ),
            ),
        )
    }

    override fun inspectRuntimeProfile(
        callback: (Result<HostRuntimeSnapshot>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    val rust =
                        try {
                            RustMobileBridge.inspectRuntime(dataRoot, securityMaterial)
                        } finally {
                            securityMaterial.fill(0)
                        }
                    Log.i(
                        RUNTIME_LOG_TAG,
                        "profile=${rust.profileVersion} " +
                            "generation=${rust.processGeneration} " +
                            "phase=${rust.activationPhase} " +
                            "grants=${rust.activeGrantCount} " +
                            "recovered=${rust.recoveredUncleanStart} " +
                            "bootstrap=${rust.bootstrapStoreOpened} " +
                            "registry=${rust.registryState} " +
                            "kql=${rust.localKqlFixtureVerified} " +
                            "planner=${rust.privatePlannerVerified} " +
                            "noLlm=${rust.noLlmProvider} " +
                            "staleFence=${rust.staleCallbackRejected} " +
                            "secure=${rust.secureProfileActive} " +
                            "binding=${rust.installationBindingVerified} " +
                            "unlocked=${rust.securitySessionUnlocked} " +
                            "vault=${rust.privateVaultReady} " +
                            "domains=${rust.identityDomainsSeparated} " +
                            "privacy=${rust.privacyDefaultsFailSafe} " +
                            "history=${rust.redactedHistoryReady} " +
                            "drafts=${rust.encryptedRawDraftCount} " +
                            "shareSpools=${rust.pendingShareSpoolCount} " +
                            "onboarding=${rust.onboardingCursor}",
                    )
                    HostRuntimeSnapshot(
                        profileVersion = rust.profileVersion,
                        processGeneration = rust.processGeneration,
                        activationPhase = rust.activationPhase,
                        activeGrantCount = rust.activeGrantCount,
                        recoveredUncleanStart = rust.recoveredUncleanStart,
                        bootstrapStoreOpened = rust.bootstrapStoreOpened,
                        registryState = rust.registryState,
                        localKqlFixtureVerified = rust.localKqlFixtureVerified,
                        privatePlannerVerified = rust.privatePlannerVerified,
                        noLlmProvider = rust.noLlmProvider,
                        staleCallbackRejected = rust.staleCallbackRejected,
                        secureProfileActive = rust.secureProfileActive,
                        installationBindingVerified = rust.installationBindingVerified,
                        installationCreated = rust.installationCreated,
                        securitySessionUnlocked = rust.securitySessionUnlocked,
                        privateVaultReady = rust.privateVaultReady,
                        identityDomainsSeparated = rust.identityDomainsSeparated,
                        privacyDefaultsFailSafe = rust.privacyDefaultsFailSafe,
                        redactedHistoryReady = rust.redactedHistoryReady,
                        encryptedRawDraftCount = rust.encryptedRawDraftCount,
                        pendingShareSpoolCount = rust.pendingShareSpoolCount,
                        onboardingCursor =
                            HostOnboardingCursor.ofRaw(rust.onboardingCursor)
                                ?: HostOnboardingCursor.WELCOME,
                    )
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Log.w(
                            RUNTIME_LOG_TAG,
                            "secure runtime open rejected",
                        )
                        Result.failure(
                            FlutterError(
                                code = "RUNTIME_OPEN_FAILED",
                                message = it.message ?: "Rust mobile runtime failed to open",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    override fun saveRawTextDraft(
        contentLanguage: String,
        content: String,
        callback: (Result<HostRawDraftReceipt>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    val receipt =
                        try {
                            RustMobileBridge.saveRawTextDraft(
                                dataRoot,
                                securityMaterial,
                                contentLanguage,
                                content,
                            )
                        } finally {
                            securityMaterial.fill(0)
                        }
                    HostRawDraftReceipt(
                        draftRef = receipt.draftRef,
                        contentLanguage = receipt.contentLanguage,
                        contentBytes = receipt.contentBytes,
                        totalDrafts = receipt.totalDrafts,
                    )
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Result.failure(
                            FlutterError(
                                code = "RAW_DRAFT_SAVE_FAILED",
                                message = "Private draft could not be saved",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    override fun inspectPendingShareSpools(
        callback: (Result<List<HostShareSpoolSummary>>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    try {
                        RustMobileBridge
                            .pendingShareSpools(dataRoot, securityMaterial)
                            .map { spool ->
                                HostShareSpoolSummary(
                                    spoolRef = spool.spoolRef,
                                    mimeType = spool.mimeType,
                                    contentBytes = spool.contentBytes,
                                    receivedAtMonotonicMillis =
                                        spool.receivedAtMonotonicMillis,
                                )
                            }
                    } finally {
                        securityMaterial.fill(0)
                    }
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Result.failure(
                            FlutterError(
                                code = "SHARE_SPOOL_INSPECT_FAILED",
                                message = "Private share spool could not be inspected",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    override fun importSharedText(
        spoolRef: String,
        contentLanguage: String,
        callback: (Result<HostRawDraftReceipt>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    val receipt =
                        try {
                            RustMobileBridge.importSharedText(
                                dataRoot,
                                securityMaterial,
                                spoolRef,
                                contentLanguage,
                            )
                        } finally {
                            securityMaterial.fill(0)
                        }
                    HostRawDraftReceipt(
                        draftRef = receipt.draftRef,
                        contentLanguage = receipt.contentLanguage,
                        contentBytes = receipt.contentBytes,
                        totalDrafts = receipt.totalDrafts,
                    )
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Result.failure(
                            FlutterError(
                                code = "SHARE_SPOOL_IMPORT_FAILED",
                                message = "Shared text could not be imported",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    override fun setOnboardingCursor(
        cursor: HostOnboardingCursor,
        callback: (Result<Boolean>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    try {
                        RustMobileBridge.setOnboardingCursor(
                            dataRoot,
                            securityMaterial,
                            cursor.raw,
                        )
                    } finally {
                        securityMaterial.fill(0)
                    }
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Result.failure(
                            FlutterError(
                                code = "ONBOARDING_CURSOR_SAVE_FAILED",
                                message = "Onboarding progress could not be saved",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    fun lockPrivateNode() {
        runtimeExecutor.execute {
            RustMobileBridge.lockRuntime()
        }
    }

    override fun startFeasibilityOperation(
        delayMilliseconds: Long,
        callback: (Result<String>) -> Unit,
    ) {
        if (delayMilliseconds !in 0..MAX_FEASIBILITY_DELAY_MILLIS) {
            callback(
                Result.failure(
                    FlutterError(
                        code = "HOST_INVALID_DELAY",
                        message = "delayMilliseconds must be between 0 and 30000",
                    ),
                ),
            )
            return
        }

        val operationId = UUID.randomUUID().toString()
        val completion = Runnable {
            if (pending.remove(operationId) != null) {
                events.emit(
                    HostOperationEvent(
                        operationId = operationId,
                        kind = HostOperationEventKind.COMPLETED,
                        code = "HOST_OPERATION_COMPLETED",
                    ),
                )
            }
        }
        pending[operationId] = completion
        events.emit(
            HostOperationEvent(
                operationId = operationId,
                kind = HostOperationEventKind.STARTED,
                code = "HOST_OPERATION_STARTED",
            ),
        )
        mainHandler.postDelayed(completion, delayMilliseconds)
        callback(Result.success(operationId))
    }

    override fun cancelFeasibilityOperation(
        operationId: String,
        callback: (Result<Boolean>) -> Unit,
    ) {
        val pendingOperation = pending.remove(operationId)
        if (pendingOperation == null) {
            callback(Result.success(false))
            return
        }
        mainHandler.removeCallbacks(pendingOperation)
        events.emit(
            HostOperationEvent(
                operationId = operationId,
                kind = HostOperationEventKind.CANCELLED,
                code = "HOST_OPERATION_CANCELLED",
            ),
        )
        callback(Result.success(true))
    }
}
