package org.onebrain.onebrain_mobile

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.StatFs
import android.util.Log
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import org.onebrain.onebrain_mobile.generated.FlutterError
import org.onebrain.onebrain_mobile.generated.HostBootstrapSnapshot
import org.onebrain.onebrain_mobile.generated.HostMediaClass
import org.onebrain.onebrain_mobile.generated.HostOwnedMediaSummary
import org.onebrain.onebrain_mobile.generated.HostOperationEvent
import org.onebrain.onebrain_mobile.generated.HostOperationEventKind
import org.onebrain.onebrain_mobile.generated.HostOperationEventsStreamHandler
import org.onebrain.onebrain_mobile.generated.HostOnboardingCursor
import org.onebrain.onebrain_mobile.generated.HostRuntimeSnapshot
import org.onebrain.onebrain_mobile.generated.HostRawDraftReceipt
import org.onebrain.onebrain_mobile.generated.HostRegistryInitAvailability
import org.onebrain.onebrain_mobile.generated.HostRegistryInitPlan
import org.onebrain.onebrain_mobile.generated.HostRegistryNetworkPolicy
import org.onebrain.onebrain_mobile.generated.HostRegistryTrustMode
import org.onebrain.onebrain_mobile.generated.MobileHostApi
import org.onebrain.onebrain_mobile.generated.PigeonEventSink
import org.onebrain.onebrain_mobile.generated.HostShareSpoolSummary

private const val HOST_API_VERSION = "8"
private const val MAX_FEASIBILITY_DELAY_MILLIS = 30_000L
private const val MEDIA_PICK_REQUEST_CODE = 7_104
private const val MEDIA_STREAM_CHUNK_BYTES = 256 * 1024
private const val RUNTIME_LOG_TAG = "OneBrainMobileRuntime"
private const val SHARE_CALLBACK_TOKEN =
    "org.onebrain.onebrain_mobile.extra.SHARE_CALLBACK_TOKEN"

class MainActivity : FlutterActivity() {
    private lateinit var hostApi: AndroidMobileHost
    private lateinit var hostEvents: AndroidHostEvents
    private var pendingMediaPick: PendingMediaPick? = null

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        hostEvents = AndroidHostEvents()
        hostApi =
            AndroidMobileHost(
                applicationContext,
                applicationContext.noBackupFilesDir.absolutePath,
                SecurityMaterialStore(applicationContext),
                hostEvents,
                ::requestPrivateMediaPick,
            )
        val messenger = flutterEngine.dartExecutor.binaryMessenger
        MobileHostApi.setUp(messenger, hostApi)
        HostOperationEventsStreamHandler.register(messenger, hostEvents)
        hostApi.acceptShareIntent(intent)
        RegistryUidtStartupReconciler.reconcileOnce(applicationContext)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (::hostApi.isInitialized) {
            hostApi.acceptShareIntent(intent)
        }
    }

    override fun onStart() {
        super.onStart()
        if (::hostApi.isInitialized) {
            hostApi.allowForegroundMediaWork()
        }
    }

    @Deprecated("Android system picker result is intentionally owned by the native host")
    override fun onActivityResult(
        requestCode: Int,
        resultCode: Int,
        data: Intent?,
    ) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != MEDIA_PICK_REQUEST_CODE) {
            return
        }
        val pending = pendingMediaPick ?: return
        pendingMediaPick = null
        val uri = data?.data
        if (resultCode != Activity.RESULT_OK || uri == null) {
            pending.callback(
                Result.failure(
                    FlutterError(
                        code = "MEDIA_PICK_CANCELLED",
                        message = "No private media source was selected",
                    ),
                ),
            )
            return
        }
        hostApi.stagePickedMedia(uri, pending.mediaClass, pending.callback)
    }

    private fun requestPrivateMediaPick(
        mediaClass: HostMediaClass,
        callback: (Result<HostOwnedMediaSummary>) -> Unit,
    ) {
        if (pendingMediaPick != null) {
            callback(
                Result.failure(
                    FlutterError(
                        code = "MEDIA_PICK_BUSY",
                        message = "Another private media picker is active",
                    ),
                ),
            )
            return
        }
        pendingMediaPick = PendingMediaPick(mediaClass, callback)
        val intent =
            Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                type =
                    when (mediaClass) {
                        HostMediaClass.IMAGE -> "image/*"
                        HostMediaClass.VIDEO -> "video/*"
                        HostMediaClass.AUDIO -> "audio/*"
                        HostMediaClass.DOCUMENT -> "application/pdf"
                    }
            }
        @Suppress("DEPRECATION")
        startActivityForResult(intent, MEDIA_PICK_REQUEST_CODE)
    }

    override fun onStop() {
        if (::hostApi.isInitialized) {
            hostApi.stopForegroundMediaWork()
            hostApi.lockPrivateNode()
        }
        super.onStop()
    }
}

private data class PendingMediaPick(
    val mediaClass: HostMediaClass,
    val callback: (Result<HostOwnedMediaSummary>) -> Unit,
)

private data class RegistryDevelopmentFixture(
    val trustProfile: ByteArray,
    val channelHead: ByteArray,
    val release: ByteArray,
)

private data class RegistryStorageFacts(
    val allocationUnitBytes: Long,
    val totalUsableBytes: Long,
    val measuredFreeBytes: Long,
)

private fun RustOwnedMediaSummary.toHostOwnedMedia() =
    HostOwnedMediaSummary(
        mediaRef = mediaRef,
        mediaClass =
            HostMediaClass.ofRaw(
                when (mediaClass) {
                    "image" -> 0
                    "video" -> 1
                    "audio" -> 2
                    "document" -> 3
                    else -> -1
                },
            ) ?: error("Rust returned an invalid media class"),
        mimeType = mimeType,
        contentBytes = contentBytes,
        verifiedBytes = verifiedBytes,
        storageClass = storageClass,
        ownedHold = ownedHold,
        importState = importState,
    )

private fun RustRegistryInitPlan.toHostRegistryPlan() =
    HostRegistryInitPlan(
        operationId = operationId,
        stateCode = stateCode.toLong(),
        channelId = channelId,
        releaseId = releaseId,
        manifestDigest = manifestDigest,
        trustProfileDigest = trustProfileDigest,
        headGeneration = headGeneration,
        releaseSequence = releaseSequence,
        publisherMinAdditionalFreeBytes = publisherMinAdditionalFreeBytes,
        artifactTotalBytes = artifactTotalBytes,
        targetTotalAllocBytes = targetTotalAllocBytes,
        transferInitialBytes = transferInitialBytes,
        verificationWorkspaceBytes = verificationWorkspaceBytes,
        catalogGrowthBytes = catalogGrowthBytes,
        safetyReserveBytes = safetyReserveBytes,
        destinationTotalUsableBytes = destinationTotalUsableBytes,
        measuredFreeBytes = measuredFreeBytes,
        initialRequiredFreeBytes = initialRequiredFreeBytes,
        admitted = admitted,
        transportEnabled = false,
        trustMode = HostRegistryTrustMode.DEVELOPMENT_FIXTURE,
    )

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
    private val context: Context,
    private val dataRoot: String,
    private val securityMaterialStore: SecurityMaterialStore,
    private val events: AndroidHostEvents,
    private val requestMediaPick:
        (HostMediaClass, (Result<HostOwnedMediaSummary>) -> Unit) -> Unit,
) : MobileHostApi {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val runtimeExecutor =
        Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "onebrain-mobile-runtime")
        }
    private val pending = mutableMapOf<String, Runnable>()
    private val foregroundMediaWorkAllowed = AtomicBoolean(true)

    override fun inspectRegistryInitAvailability(
        callback: (Result<HostRegistryInitAvailability>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val rust = RustMobileBridge.inspectBootstrap()
            val fixture = loadRegistryDevelopmentFixture()
            val available = rust.linked && rust.abiVersion >= 8 && fixture != null
            val reason =
                when {
                    !rust.linked -> "RUST_BRIDGE_UNAVAILABLE"
                    rust.abiVersion < 8 -> "REGISTRY_ABI_UNAVAILABLE"
                    fixture == null -> "PRODUCTION_TRUST_PROFILE_UNAVAILABLE"
                    else -> "DEVELOPMENT_FIXTURE_READY"
                }
            val availability =
                HostRegistryInitAvailability(
                    available = available,
                    trustMode =
                        if (available) {
                            HostRegistryTrustMode.DEVELOPMENT_FIXTURE
                        } else {
                            HostRegistryTrustMode.UNAVAILABLE
                        },
                    channelId = "stable",
                    reasonCode = reason,
                    transportEnabled = false,
                )
            mainHandler.post { callback(Result.success(availability)) }
        }
    }

    override fun beginRegistryInit(
        channelId: String,
        callback: (Result<HostRegistryInitPlan>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    check(channelId == "stable") { "Unsupported Registry channel" }
                    val fixture = checkNotNull(loadRegistryDevelopmentFixture()) {
                        "No signed Registry trust source is available"
                    }
                    val storage = registryStorageFacts()
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    try {
                        RustMobileBridge.prepareRegistryInit(
                            dataRoot = dataRoot,
                            securityMaterial = securityMaterial,
                            channelId = channelId,
                            trustProfile = fixture.trustProfile,
                            channelHead = fixture.channelHead,
                            release = fixture.release,
                            allocationUnitBytes = storage.allocationUnitBytes,
                            destinationTotalUsableBytes = storage.totalUsableBytes,
                            measuredFreeBytes = storage.measuredFreeBytes,
                        ).toHostRegistryPlan()
                    } finally {
                        securityMaterial.fill(0)
                    }
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Result.failure(
                            FlutterError(
                                code = "REGISTRY_INIT_PLAN_FAILED",
                                message = it.message ?: "Signed Registry Init plan was rejected",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    override fun deferRegistryInit(
        operationId: String,
        manifestDigest: String,
        callback: (Result<Boolean>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    check(RustMobileBridge.deferRegistryInit(operationId, manifestDigest)) {
                        "Rust rejected the exact Init defer receipt"
                    }
                    true
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Result.failure(
                            FlutterError(
                                code = "REGISTRY_INIT_DEFER_FAILED",
                                message = it.message ?: "Init could not enter Limited mode",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    override fun confirmRegistryInit(
        operationId: String,
        manifestDigest: String,
        networkPolicy: HostRegistryNetworkPolicy,
        oneTimeNetworkOverride: Boolean,
        callback: (Result<HostRegistryInitPlan>) -> Unit,
    ) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    check(
                        !oneTimeNetworkOverride ||
                            networkPolicy == HostRegistryNetworkPolicy.ANY_NETWORK,
                    ) { "One-time Registry override requires Any network policy" }
                    val fixture = checkNotNull(loadRegistryDevelopmentFixture()) {
                        "No signed Registry trust source is available"
                    }
                    val storage = registryStorageFacts()
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    try {
                        RustMobileBridge.confirmRegistryInit(
                            dataRoot = dataRoot,
                            securityMaterial = securityMaterial,
                            operationId = operationId,
                            manifestDigest = manifestDigest,
                            trustProfile = fixture.trustProfile,
                            networkPolicyCode = networkPolicy.raw,
                            oneTimeNetworkOverride = oneTimeNetworkOverride,
                            allocationUnitBytes = storage.allocationUnitBytes,
                            destinationTotalUsableBytes = storage.totalUsableBytes,
                            measuredFreeBytes = storage.measuredFreeBytes,
                        ).toHostRegistryPlan()
                    } finally {
                        securityMaterial.fill(0)
                    }
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Result.failure(
                            FlutterError(
                                code = "REGISTRY_INIT_CONFIRM_FAILED",
                                message = it.message ?: "Exact Registry Init confirmation failed",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    private fun registryStorageFacts(): RegistryStorageFacts {
        val stats = StatFs(dataRoot)
        return RegistryStorageFacts(
            allocationUnitBytes = stats.blockSizeLong,
            totalUsableBytes = stats.totalBytes,
            measuredFreeBytes = stats.availableBytes,
        )
    }

    private fun loadRegistryDevelopmentFixture(): RegistryDevelopmentFixture? =
        runCatching {
            RegistryDevelopmentFixture(
                trustProfile = readHexAsset("mob05a/registry_trust_profile.cbor.hex"),
                channelHead = readHexAsset("mob05a/registry_channel_head.cbor.hex"),
                release = readHexAsset("mob05a/registry_release.cbor.hex"),
            )
        }.getOrNull()

    private fun readHexAsset(name: String): ByteArray {
        val value = context.assets.open(name).bufferedReader().use { it.readText() }.trim()
        require(value.isNotEmpty() && value.length % 2 == 0 && value.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) {
            "Invalid development Registry fixture"
        }
        return ByteArray(value.length / 2) { index ->
            value.substring(index * 2, index * 2 + 2).toInt(16).toByte()
        }
    }

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
                            "stagedMedia=${rust.stagedVerifiedMediaCount} " +
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
                        stagedVerifiedMediaCount = rust.stagedVerifiedMediaCount,
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

    override fun pickAndImportOwnedMedia(
        mediaClass: HostMediaClass,
        callback: (Result<HostOwnedMediaSummary>) -> Unit,
    ) {
        mainHandler.post {
            requestMediaPick(mediaClass, callback)
        }
    }

    fun stagePickedMedia(
        uri: Uri,
        mediaClass: HostMediaClass,
        callback: (Result<HostOwnedMediaSummary>) -> Unit,
    ) {
        runtimeExecutor.execute {
            var sourceRef: String? = null
            val result =
                runCatching {
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    try {
                        val declaredMimeType =
                            context.contentResolver.getType(uri)?.lowercase()
                                ?: "application/octet-stream"
                        sourceRef =
                            RustMobileBridge.startMediaStage(
                                dataRoot,
                                securityMaterial,
                                mediaClass.name.lowercase(),
                                declaredMimeType,
                            )
                        val buffer = ByteArray(MEDIA_STREAM_CHUNK_BYTES)
                        try {
                            context.contentResolver.openInputStream(uri).use { input ->
                                checkNotNull(input) {
                                    "Android content provider returned no readable media stream"
                                }
                                while (true) {
                                    check(foregroundMediaWorkAllowed.get()) {
                                        "Private media staging lost its foreground grant"
                                    }
                                    val read = input.read(buffer)
                                    if (read < 0) {
                                        break
                                    }
                                    check(foregroundMediaWorkAllowed.get()) {
                                        "Private media staging lost its foreground grant"
                                    }
                                    if (read == 0) {
                                        continue
                                    }
                                    val chunk =
                                        if (read == buffer.size) {
                                            buffer
                                        } else {
                                            buffer.copyOf(read)
                                        }
                                    try {
                                        RustMobileBridge.appendMediaStage(sourceRef!!, chunk)
                                    } finally {
                                        if (chunk !== buffer) {
                                            chunk.fill(0)
                                        }
                                    }
                                }
                            }
                        } finally {
                            buffer.fill(0)
                        }
                        RustMobileBridge.finishOwnedMediaImport(sourceRef!!).toHostOwnedMedia()
                    } finally {
                        securityMaterial.fill(0)
                    }
                }.fold(
                    onSuccess = {
                        Log.i(
                            RUNTIME_LOG_TAG,
                            "owned_original_media_imported class=${mediaClass.name.lowercase()}",
                        )
                        Result.success(it)
                    },
                    onFailure = {
                        sourceRef?.let { reference ->
                            runCatching { RustMobileBridge.abortMediaStage(reference) }
                        }
                        Log.w(RUNTIME_LOG_TAG, "private_media_stage_rejected")
                        Result.failure(
                            FlutterError(
                                code = "MEDIA_STAGE_FAILED",
                                message = "Private media could not be verified and staged",
                            ),
                        )
                    },
                )
            mainHandler.post { callback(result) }
        }
    }

    override fun inspectOwnedMedia(callback: (Result<List<HostOwnedMediaSummary>>) -> Unit) {
        runtimeExecutor.execute {
            val result =
                runCatching {
                    val securityMaterial = securityMaterialStore.loadOrCreate()
                    try {
                        RustMobileBridge.ownedMedia(dataRoot, securityMaterial).map {
                            it.toHostOwnedMedia()
                        }
                    } finally {
                        securityMaterial.fill(0)
                    }
                }.fold(
                    onSuccess = { Result.success(it) },
                    onFailure = {
                        Result.failure(
                            FlutterError(
                                code = "OWNED_MEDIA_INSPECTION_FAILED",
                                message = "Owned media could not be inspected",
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

    fun allowForegroundMediaWork() {
        foregroundMediaWorkAllowed.set(true)
    }

    fun stopForegroundMediaWork() {
        foregroundMediaWorkAllowed.set(false)
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
