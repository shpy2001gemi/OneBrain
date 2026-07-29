package org.onebrain.onebrain_mobile

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
import org.onebrain.onebrain_mobile.generated.HostRuntimeSnapshot
import org.onebrain.onebrain_mobile.generated.MobileHostApi
import org.onebrain.onebrain_mobile.generated.PigeonEventSink

private const val HOST_API_VERSION = "2"
private const val MAX_FEASIBILITY_DELAY_MILLIS = 30_000L
private const val RUNTIME_LOG_TAG = "OneBrainMobileRuntime"

class MainActivity : FlutterActivity() {
    private lateinit var hostApi: AndroidMobileHost
    private lateinit var hostEvents: AndroidHostEvents

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        hostEvents = AndroidHostEvents()
        hostApi = AndroidMobileHost(applicationContext.noBackupFilesDir.absolutePath, hostEvents)
        val messenger = flutterEngine.dartExecutor.binaryMessenger
        MobileHostApi.setUp(messenger, hostApi)
        HostOperationEventsStreamHandler.register(messenger, hostEvents)
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
    private val events: AndroidHostEvents,
) : MobileHostApi {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val runtimeExecutor =
        Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "onebrain-mobile-runtime")
        }
    private val pending = mutableMapOf<String, Runnable>()

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
                    val rust = RustMobileBridge.inspectRuntime(dataRoot)
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
                            "staleFence=${rust.staleCallbackRejected}",
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
                    )
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
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
