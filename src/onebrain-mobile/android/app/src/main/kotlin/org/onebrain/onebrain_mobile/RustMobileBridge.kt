package org.onebrain.onebrain_mobile

private const val RUST_LIBRARY_NAME = "onebrain_mobile_bridge"
private const val RUST_ROUND_TRIP_NONCE = 0x4F_42_4D_30_31L

internal data class RustBootstrapFacts(
    val linked: Boolean,
    val coreVersion: String,
    val abiVersion: Long,
    val registryRequestIssued: Boolean,
    val roundTripVerified: Boolean,
)

internal data class RustRuntimeFacts(
    val profileVersion: String,
    val processGeneration: Long,
    val activationPhase: String,
    val activeGrantCount: Long,
    val recoveredUncleanStart: Boolean,
    val bootstrapStoreOpened: Boolean,
    val registryState: String,
    val localKqlFixtureVerified: Boolean,
    val privatePlannerVerified: Boolean,
    val noLlmProvider: Boolean,
    val staleCallbackRejected: Boolean,
    val secureProfileActive: Boolean,
    val installationBindingVerified: Boolean,
    val installationCreated: Boolean,
    val securitySessionUnlocked: Boolean,
    val privateVaultReady: Boolean,
    val identityDomainsSeparated: Boolean,
    val privacyDefaultsFailSafe: Boolean,
    val redactedHistoryReady: Boolean,
    val encryptedRawDraftCount: Long,
    val pendingShareSpoolCount: Long,
    val stagedVerifiedMediaCount: Long,
    val onboardingCursor: Int,
)

internal data class RustRawDraftReceipt(
    val draftRef: String,
    val contentLanguage: String,
    val contentBytes: Long,
    val totalDrafts: Long,
)

internal data class RustShareSpoolSummary(
    val spoolRef: String,
    val mimeType: String,
    val contentBytes: Long,
    val receivedAtMonotonicMillis: Long,
)

internal data class RustMediaStageReceipt(
    val sourceRef: String,
    val mediaClass: String,
    val mimeType: String,
    val contentBytes: Long,
    val blake3Digest: String,
)

internal data class RustOwnedMediaSummary(
    val mediaRef: String,
    val mediaClass: String,
    val mimeType: String,
    val contentBytes: Long,
    val verifiedBytes: Long,
    val storageClass: String,
    val ownedHold: Boolean,
    val importState: String,
)

internal data class RustRegistryInitPlan(
    val operationId: String,
    val stateCode: Int,
    val channelId: String,
    val releaseId: String,
    val manifestDigest: String,
    val trustProfileDigest: String,
    val headGeneration: Long,
    val releaseSequence: Long,
    val publisherMinAdditionalFreeBytes: Long,
    val artifactTotalBytes: Long,
    val targetTotalAllocBytes: Long,
    val transferInitialBytes: Long,
    val verificationWorkspaceBytes: Long,
    val catalogGrowthBytes: Long,
    val safetyReserveBytes: Long,
    val destinationTotalUsableBytes: Long,
    val measuredFreeBytes: Long,
    val initialRequiredFreeBytes: Long,
    val admitted: Boolean,
)

internal data class RustRegistryTransferSchedule(
    val transferNonce: String,
    val operationId: String,
    val releaseId: String,
    val manifestDigest: String,
    val trustProfileDigest: String,
    val requestFingerprint: String,
    val sourcePlanDigest: String,
    val expectedTotalBytes: Long,
    val sourceKindCode: Int,
    val platformCode: Int,
    val androidJobId: Int?,
    val osTransferId: String?,
    val stateCode: Int,
    val preparedProcessGeneration: Long,
    val submittedProcessGeneration: Long?,
    val adoptedProcessGeneration: Long?,
)

internal data class RustRegistryLandingProgress(
    val transferNonce: String,
    val totalChunks: Int,
    val verifiedChunks: Int,
    val expectedBytes: Long,
    val verifiedBytes: Long,
    val bytesComplete: Boolean,
)

internal data class RustRegistryChunkSourceTarget(
    val transferNonce: String,
    val artifactRole: Int,
    val chunkIndex: Int,
    val artifactSourceOffset: Long,
    val expectedLength: Long,
    val resumeOffset: Long,
)

internal data class RustRegistryChunkWriteReceipt(
    val transferNonce: String,
    val releaseId: String,
    val artifactRole: Int,
    val chunkIndex: Int,
    val expectedBytes: Long,
    val writtenBytes: Long,
    val durableBytes: Long,
    val stateCode: Int,
)

internal object RustMobileBridge {
    private const val NO_ACTIVE_REGISTRY_TRANSFER_STATUS = 12
    private val loadFailure =
        runCatching { System.loadLibrary(RUST_LIBRARY_NAME) }.exceptionOrNull()

    fun inspectBootstrap(): RustBootstrapFacts {
        if (loadFailure != null) {
            return RustBootstrapFacts(
                linked = false,
                coreVersion = "unavailable",
                abiVersion = 0,
                registryRequestIssued = false,
                roundTripVerified = false,
            )
        }
        return RustBootstrapFacts(
            linked = true,
            coreVersion = nativeCoreVersion(),
            abiVersion = nativeAbiVersion().toLong(),
            registryRequestIssued = nativeRegistryRequestIssued(),
            roundTripVerified =
                nativeRoundTrip(RUST_ROUND_TRIP_NONCE) == RUST_ROUND_TRIP_NONCE,
        )
    }

    fun inspectRuntime(
        dataRoot: String,
        securityMaterial: ByteArray,
    ): RustRuntimeFacts {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        val statusCode = nativeRuntimeOpenSecure(dataRoot, securityMaterial)
        check(statusCode == 0) {
            "Rust mobile runtime failed to open with status $statusCode"
        }
        return RustRuntimeFacts(
            profileVersion = "MOB-04/3",
            processGeneration = nativeRuntimeProcessGeneration(),
            activationPhase =
                when (nativeRuntimeActivationPhase()) {
                    0 -> "Dormant"
                    1 -> "Starting"
                    2 -> "Active"
                    3 -> "Draining"
                    else -> "Unknown"
                },
            activeGrantCount = nativeRuntimeActiveGrantCount().toLong(),
            recoveredUncleanStart = nativeRuntimeRecoveredUncleanStart(),
            bootstrapStoreOpened = nativeRuntimeBootstrapStoreOpened(),
            registryState =
                if (nativeRuntimeRegistryBootstrapOnly()) {
                    "BootstrapOnly"
                } else {
                    "Unknown"
                },
            localKqlFixtureVerified = nativeRuntimeLocalKqlFixtureVerified(),
            privatePlannerVerified = nativeRuntimePrivatePlannerVerified(),
            noLlmProvider = nativeRuntimeNoLlmProvider(),
            staleCallbackRejected = nativeRuntimeStaleCallbackRejected(),
            secureProfileActive = nativeRuntimeSecureProfileActive(),
            installationBindingVerified = nativeRuntimeInstallationBindingVerified(),
            installationCreated = nativeRuntimeInstallationCreated(),
            securitySessionUnlocked = nativeRuntimeSecuritySessionUnlocked(),
            privateVaultReady = nativeRuntimePrivateVaultReady(),
            identityDomainsSeparated = nativeRuntimeIdentityDomainsSeparated(),
            privacyDefaultsFailSafe = nativeRuntimePrivacyDefaultsFailSafe(),
            redactedHistoryReady = nativeRuntimeRedactedHistoryReady(),
            encryptedRawDraftCount = nativeRuntimeEncryptedRawDraftCount(),
            pendingShareSpoolCount = nativeRuntimePendingShareSpoolCount(),
            stagedVerifiedMediaCount = nativeRuntimeStagedVerifiedMediaCount(),
            onboardingCursor = nativeRuntimeOnboardingCursor(),
        )
    }

    fun prepareRegistryInit(
        dataRoot: String,
        securityMaterial: ByteArray,
        channelId: String,
        trustProfile: ByteArray,
        channelHead: ByteArray,
        release: ByteArray,
        allocationUnitBytes: Long,
        destinationTotalUsableBytes: Long,
        measuredFreeBytes: Long,
    ): RustRegistryInitPlan {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryPlan(
            nativeRuntimePrepareRegistryInit(
                channelId,
                trustProfile,
                channelHead,
                release,
                allocationUnitBytes,
                destinationTotalUsableBytes,
                measuredFreeBytes,
            ),
        )
    }

    fun deferRegistryInit(
        operationId: String,
        manifestDigest: String,
    ): Boolean = nativeRuntimeDeferRegistryInit(operationId, manifestDigest) == 0

    fun confirmRegistryInit(
        dataRoot: String,
        securityMaterial: ByteArray,
        operationId: String,
        manifestDigest: String,
        trustProfile: ByteArray,
        networkPolicyCode: Int,
        oneTimeNetworkOverride: Boolean,
        allocationUnitBytes: Long,
        destinationTotalUsableBytes: Long,
        measuredFreeBytes: Long,
    ): RustRegistryInitPlan {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryPlan(
            nativeRuntimeConfirmRegistryInit(
                operationId,
                manifestDigest,
                trustProfile,
                networkPolicyCode,
                oneTimeNetworkOverride,
                allocationUnitBytes,
                destinationTotalUsableBytes,
                measuredFreeBytes,
            ),
        )
    }

    fun prepareRegistryTransferSchedule(
        dataRoot: String,
        securityMaterial: ByteArray,
        operationId: String,
        manifestDigest: String,
        platformCode: Int,
        sourceKindCode: Int,
        requestFingerprint: String,
        sourcePlanDigest: String,
        expectedTotalBytes: Long,
        foregroundUserResume: Boolean,
    ): RustRegistryTransferSchedule {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryTransferSchedule(
            nativeRuntimePrepareRegistryTransferSchedule(
                operationId,
                manifestDigest,
                platformCode,
                sourceKindCode,
                requestFingerprint,
                sourcePlanDigest,
                expectedTotalBytes,
                foregroundUserResume,
            ),
        )
    }

    fun prepareRegistryLocalImportSchedule(
        dataRoot: String,
        securityMaterial: ByteArray,
        operationId: String,
        manifestDigest: String,
        foregroundUserResume: Boolean,
    ): RustRegistryTransferSchedule {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryTransferSchedule(
            nativeRuntimePrepareRegistryLocalImportSchedule(
                operationId,
                manifestDigest,
                foregroundUserResume,
            ),
        )
    }

    fun markRegistryTransferSubmitted(
        dataRoot: String,
        securityMaterial: ByteArray,
        transferNonce: String,
        osTransferId: String,
    ): RustRegistryTransferSchedule {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryTransferSchedule(
            nativeRuntimeMarkRegistryTransferSubmitted(transferNonce, osTransferId),
        )
    }

    fun adoptRegistryTransfer(
        dataRoot: String,
        securityMaterial: ByteArray,
        transferNonce: String,
        osTransferId: String,
        observedRequestFingerprint: String,
        observedAndroidJobId: Int?,
        matchingTaskCount: Int,
    ): RustRegistryTransferSchedule {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryTransferSchedule(
            nativeRuntimeAdoptRegistryTransfer(
                transferNonce,
                osTransferId,
                observedRequestFingerprint,
                observedAndroidJobId?.toLong() ?: -1L,
                matchingTaskCount,
            ),
        )
    }

    fun recordRegistryTransferMissing(
        dataRoot: String,
        securityMaterial: ByteArray,
        transferNonce: String,
        positiveUserStopEvidence: Boolean,
    ): RustRegistryTransferSchedule {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryTransferSchedule(
            nativeRuntimeRecordRegistryTransferMissing(
                transferNonce,
                positiveUserStopEvidence,
            ),
        )
    }

    fun registryTransferScheduleForChannel(
        dataRoot: String,
        securityMaterial: ByteArray,
        channelId: String,
    ): RustRegistryTransferSchedule? {
        ensureSecureRuntime(dataRoot, securityMaterial)
        val encoded = nativeRuntimeRegistryTransferScheduleForChannel(channelId)
        if (encoded == "ERR:$NO_ACTIVE_REGISTRY_TRANSFER_STATUS") {
            return null
        }
        return decodeRegistryTransferSchedule(encoded)
    }

    fun prepareRegistryChunkLedger(
        dataRoot: String,
        securityMaterial: ByteArray,
        transferNonce: String,
    ): RustRegistryLandingProgress {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryLandingProgress(
            nativeRuntimePrepareRegistryChunkLedger(transferNonce),
        )
    }

    fun recoverRegistryChunkLedger(
        dataRoot: String,
        securityMaterial: ByteArray,
        transferNonce: String,
    ): RustRegistryLandingProgress {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryLandingProgress(
            nativeRuntimeRecoverRegistryChunkLedger(transferNonce),
        )
    }

    fun registryLandingProgress(
        dataRoot: String,
        securityMaterial: ByteArray,
        transferNonce: String,
    ): RustRegistryLandingProgress {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryLandingProgress(
            nativeRuntimeRegistryLandingProgress(transferNonce),
        )
    }

    fun nextRegistryChunkSourceTarget(
        dataRoot: String,
        securityMaterial: ByteArray,
        transferNonce: String,
        artifactRole: Int,
    ): RustRegistryChunkSourceTarget? {
        ensureSecureRuntime(dataRoot, securityMaterial)
        val encoded = nativeRuntimeNextRegistryChunkSourceTarget(transferNonce, artifactRole)
        if (encoded == "NONE") return null
        check(!encoded.startsWith("ERR:")) {
            "Rust Registry source target rejected the operation (${encoded.removePrefix("ERR:")})"
        }
        val fields = encoded.split('|')
        check(fields.size == 6) {
            "Rust mobile runtime returned an invalid Registry source target"
        }
        return RustRegistryChunkSourceTarget(
            transferNonce = fields[0],
            artifactRole = fields[1].toInt(),
            chunkIndex = fields[2].toInt(),
            artifactSourceOffset = fields[3].toLong(),
            expectedLength = fields[4].toLong(),
            resumeOffset = fields[5].toLong(),
        )
    }

    fun beginRegistryChunkWrite(
        dataRoot: String,
        securityMaterial: ByteArray,
        transferNonce: String,
        artifactRole: Int,
        chunkIndex: Long,
        sourceOffset: Long,
    ): RustRegistryChunkWriteReceipt {
        ensureSecureRuntime(dataRoot, securityMaterial)
        return decodeRegistryChunkWriteReceipt(
            nativeRuntimeBeginRegistryChunkWrite(
                transferNonce,
                artifactRole,
                chunkIndex,
                sourceOffset,
            ),
        )
    }

    fun appendRegistryChunkWrite(block: ByteArray): RustRegistryChunkWriteReceipt =
        decodeRegistryChunkWriteReceipt(nativeRuntimeAppendRegistryChunkWrite(block))

    fun checkpointRegistryChunkWrite(): RustRegistryChunkWriteReceipt =
        decodeRegistryChunkWriteReceipt(nativeRuntimeCheckpointRegistryChunkWrite())

    fun finishRegistryChunkWrite(): RustRegistryChunkWriteReceipt =
        decodeRegistryChunkWriteReceipt(nativeRuntimeFinishRegistryChunkWrite())

    fun suspendRegistryChunkWrite(): RustRegistryChunkWriteReceipt =
        decodeRegistryChunkWriteReceipt(nativeRuntimeSuspendRegistryChunkWrite())

    private fun ensureSecureRuntime(
        dataRoot: String,
        securityMaterial: ByteArray,
    ) {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        check(nativeRuntimeOpenSecure(dataRoot, securityMaterial) == 0) {
            "Rust mobile runtime rejected the protected session"
        }
    }

    private fun decodeRegistryPlan(encoded: String): RustRegistryInitPlan {
        check(!encoded.startsWith("ERR:")) {
            "Rust Registry Init rejected the operation (${encoded.removePrefix("ERR:")})"
        }
        val fields = encoded.split('|')
        check(fields.size == 19) {
            "Rust mobile runtime returned an invalid Registry Init plan"
        }
        return RustRegistryInitPlan(
            operationId = fields[0],
            stateCode = fields[1].toInt(),
            channelId = fields[2],
            releaseId = fields[3],
            manifestDigest = fields[4],
            trustProfileDigest = fields[5],
            headGeneration = fields[6].toLong(),
            releaseSequence = fields[7].toLong(),
            publisherMinAdditionalFreeBytes = fields[8].toLong(),
            artifactTotalBytes = fields[9].toLong(),
            targetTotalAllocBytes = fields[10].toLong(),
            transferInitialBytes = fields[11].toLong(),
            verificationWorkspaceBytes = fields[12].toLong(),
            catalogGrowthBytes = fields[13].toLong(),
            safetyReserveBytes = fields[14].toLong(),
            destinationTotalUsableBytes = fields[15].toLong(),
            measuredFreeBytes = fields[16].toLong(),
            initialRequiredFreeBytes = fields[17].toLong(),
            admitted = fields[18] == "1",
        )
    }

    private fun decodeRegistryTransferSchedule(encoded: String): RustRegistryTransferSchedule {
        check(!encoded.startsWith("ERR:")) {
            "Rust Registry transfer barrier rejected the operation (${encoded.removePrefix("ERR:")})"
        }
        val fields = encoded.split('|')
        check(fields.size == 19) {
            "Rust mobile runtime returned an invalid Registry transfer schedule"
        }
        return RustRegistryTransferSchedule(
            transferNonce = fields[0],
            operationId = fields[1],
            releaseId = fields[2],
            manifestDigest = fields[3],
            trustProfileDigest = fields[4],
            requestFingerprint = fields[5],
            sourcePlanDigest = fields[6],
            expectedTotalBytes = fields[7].toLong(),
            sourceKindCode = fields[8].toInt(),
            platformCode = fields[9].toInt(),
            androidJobId = fields[10].toInt().takeIf { fields[11] == "1" },
            osTransferId = fields[12].ifEmpty { null },
            stateCode = fields[13].toInt(),
            preparedProcessGeneration = fields[14].toLong(),
            submittedProcessGeneration = fields[15].toLong().takeIf { fields[16] == "1" },
            adoptedProcessGeneration = fields[17].toLong().takeIf { fields[18] == "1" },
        )
    }

    private fun decodeRegistryLandingProgress(encoded: String): RustRegistryLandingProgress {
        check(!encoded.startsWith("ERR:")) {
            "Rust Registry landing rejected the operation (${encoded.removePrefix("ERR:")})"
        }
        val fields = encoded.split('|')
        check(fields.size == 6) {
            "Rust mobile runtime returned invalid Registry landing progress"
        }
        return RustRegistryLandingProgress(
            transferNonce = fields[0],
            totalChunks = fields[1].toInt(),
            verifiedChunks = fields[2].toInt(),
            expectedBytes = fields[3].toLong(),
            verifiedBytes = fields[4].toLong(),
            bytesComplete = fields[5] == "1",
        )
    }

    private fun decodeRegistryChunkWriteReceipt(encoded: String): RustRegistryChunkWriteReceipt {
        check(!encoded.startsWith("ERR:")) {
            "Rust Registry chunk writer rejected the operation (${encoded.removePrefix("ERR:")})"
        }
        val fields = encoded.split('|')
        check(fields.size == 8) {
            "Rust mobile runtime returned an invalid Registry chunk receipt"
        }
        return RustRegistryChunkWriteReceipt(
            transferNonce = fields[0],
            releaseId = fields[1],
            artifactRole = fields[2].toInt(),
            chunkIndex = fields[3].toInt(),
            expectedBytes = fields[4].toLong(),
            writtenBytes = fields[5].toLong(),
            durableBytes = fields[6].toLong(),
            stateCode = fields[7].toInt(),
        )
    }

    fun setOnboardingCursor(
        dataRoot: String,
        securityMaterial: ByteArray,
        cursor: Int,
    ): Boolean {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        check(nativeRuntimeOpenSecure(dataRoot, securityMaterial) == 0) {
            "Rust mobile runtime rejected the protected session"
        }
        return nativeRuntimeSetOnboardingCursor(cursor) == 0
    }

    fun saveRawTextDraft(
        dataRoot: String,
        securityMaterial: ByteArray,
        contentLanguage: String,
        content: String,
    ): RustRawDraftReceipt {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        check(nativeRuntimeOpenSecure(dataRoot, securityMaterial) == 0) {
            "Rust mobile runtime rejected the protected session"
        }
        val draftRef = nativeRuntimeSaveRawTextDraft(contentLanguage, content)
        check(draftRef.isNotEmpty()) {
            "Rust mobile runtime rejected the private raw draft"
        }
        return RustRawDraftReceipt(
            draftRef = draftRef,
            contentLanguage = contentLanguage.lowercase(),
            contentBytes = content.toByteArray(Charsets.UTF_8).size.toLong(),
            totalDrafts = nativeRuntimeEncryptedRawDraftCount(),
        )
    }

    fun enqueueSharedText(
        dataRoot: String,
        securityMaterial: ByteArray,
        callbackToken: String,
        mimeType: String,
        content: String,
    ): String {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        check(nativeRuntimeOpenSecure(dataRoot, securityMaterial) == 0) {
            "Rust mobile runtime rejected the protected session"
        }
        return nativeRuntimeEnqueueSharedText(callbackToken, mimeType, content).also {
            check(it.isNotEmpty()) {
                "Rust mobile runtime rejected the private share spool"
            }
        }
    }

    fun pendingShareSpools(
        dataRoot: String,
        securityMaterial: ByteArray,
    ): List<RustShareSpoolSummary> {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        check(nativeRuntimeOpenSecure(dataRoot, securityMaterial) == 0) {
            "Rust mobile runtime rejected the protected session"
        }
        val count = nativeRuntimePendingShareSpoolCount().coerceIn(0, 64).toInt()
        return (0 until count).map { index ->
            val fields = nativeRuntimePendingShareSpoolEntry(index).split('|')
            check(fields.size == 4) {
                "Rust mobile runtime returned an invalid share spool entry"
            }
            RustShareSpoolSummary(
                spoolRef = fields[0],
                mimeType = fields[1],
                contentBytes = fields[2].toLong(),
                receivedAtMonotonicMillis = fields[3].toLong(),
            )
        }
    }

    fun importSharedText(
        dataRoot: String,
        securityMaterial: ByteArray,
        spoolRef: String,
        contentLanguage: String,
    ): RustRawDraftReceipt {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        check(nativeRuntimeOpenSecure(dataRoot, securityMaterial) == 0) {
            "Rust mobile runtime rejected the protected session"
        }
        val contentBytes =
            pendingShareSpools(dataRoot, securityMaterial)
                .firstOrNull { it.spoolRef == spoolRef }
                ?.contentBytes
                ?: 0
        val draftRef = nativeRuntimeImportSharedText(spoolRef, contentLanguage)
        check(draftRef.isNotEmpty()) {
            "Rust mobile runtime rejected the private share import"
        }
        return RustRawDraftReceipt(
            draftRef = draftRef,
            contentLanguage = contentLanguage.lowercase(),
            contentBytes = contentBytes,
            totalDrafts = nativeRuntimeEncryptedRawDraftCount(),
        )
    }

    fun startMediaStage(
        dataRoot: String,
        securityMaterial: ByteArray,
        mediaClass: String,
        declaredMimeType: String,
    ): String {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        check(nativeRuntimeOpenSecure(dataRoot, securityMaterial) == 0) {
            "Rust mobile runtime rejected the protected session"
        }
        return nativeRuntimeStartMediaStage(mediaClass, declaredMimeType).also {
            check(it.isNotEmpty()) {
                "Rust mobile runtime rejected the private media stage"
            }
        }
    }

    fun appendMediaStage(
        sourceRef: String,
        chunk: ByteArray,
    ) {
        check(nativeRuntimeAppendMediaStage(sourceRef, chunk) == 0) {
            "Rust mobile runtime rejected a private media chunk"
        }
    }

    fun finishMediaStage(sourceRef: String): RustMediaStageReceipt {
        val fields = nativeRuntimeFinishMediaStage(sourceRef).split('|')
        check(fields.size == 5) {
            "Rust mobile runtime rejected the completed media stage"
        }
        return RustMediaStageReceipt(
            sourceRef = fields[0],
            mediaClass = fields[1],
            mimeType = fields[2],
            contentBytes = fields[3].toLong(),
            blake3Digest = fields[4],
        )
    }

    fun finishOwnedMediaImport(sourceRef: String): RustOwnedMediaSummary =
        decodeOwnedMedia(nativeRuntimeFinishOwnedMediaImport(sourceRef))

    fun ownedMedia(
        dataRoot: String,
        securityMaterial: ByteArray,
    ): List<RustOwnedMediaSummary> {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        check(nativeRuntimeOpenSecure(dataRoot, securityMaterial) == 0) {
            "Rust mobile runtime rejected the protected session"
        }
        val count = nativeRuntimeOwnedMediaCount().coerceIn(0, 100).toInt()
        return (0 until count).map { index ->
            decodeOwnedMedia(nativeRuntimeOwnedMediaEntry(index))
        }
    }

    private fun decodeOwnedMedia(encoded: String): RustOwnedMediaSummary {
        val fields = encoded.split('|')
        check(fields.size == 8) {
            "Rust mobile runtime returned an invalid OwnedOriginal entry"
        }
        return RustOwnedMediaSummary(
            mediaRef = fields[0],
            mediaClass = fields[1],
            mimeType = fields[2],
            contentBytes = fields[3].toLong(),
            verifiedBytes = fields[4].toLong(),
            storageClass = fields[5],
            ownedHold = fields[6] == "1",
            importState = fields[7],
        )
    }

    fun abortMediaStage(sourceRef: String) {
        check(nativeRuntimeAbortMediaStage(sourceRef) == 0) {
            "Rust mobile runtime could not abort the private media stage"
        }
    }

    fun lockRuntime(): Boolean {
        if (loadFailure != null) {
            return false
        }
        return nativeRuntimeLock() == 0
    }

    @JvmStatic private external fun nativeAbiVersion(): Int

    @JvmStatic private external fun nativeCoreVersion(): String

    @JvmStatic private external fun nativeRegistryRequestIssued(): Boolean

    @JvmStatic private external fun nativeRoundTrip(nonce: Long): Long

    @JvmStatic private external fun nativeRuntimeOpenSecure(
        dataRoot: String,
        securityMaterial: ByteArray,
    ): Int

    @JvmStatic private external fun nativeRuntimeLock(): Int

    @JvmStatic private external fun nativeRuntimePrepareRegistryInit(
        channelId: String,
        trustProfile: ByteArray,
        channelHead: ByteArray,
        release: ByteArray,
        allocationUnitBytes: Long,
        destinationTotalUsableBytes: Long,
        measuredFreeBytes: Long,
    ): String

    @JvmStatic private external fun nativeRuntimeDeferRegistryInit(
        operationId: String,
        manifestDigest: String,
    ): Int

    @JvmStatic private external fun nativeRuntimeConfirmRegistryInit(
        operationId: String,
        manifestDigest: String,
        trustProfile: ByteArray,
        networkPolicyCode: Int,
        oneTimeNetworkOverride: Boolean,
        allocationUnitBytes: Long,
        destinationTotalUsableBytes: Long,
        measuredFreeBytes: Long,
    ): String

    @JvmStatic private external fun nativeRuntimePrepareRegistryTransferSchedule(
        operationId: String,
        manifestDigest: String,
        platformCode: Int,
        sourceKindCode: Int,
        requestFingerprint: String,
        sourcePlanDigest: String,
        expectedTotalBytes: Long,
        foregroundUserResume: Boolean,
    ): String

    @JvmStatic private external fun nativeRuntimePrepareRegistryLocalImportSchedule(
        operationId: String,
        manifestDigest: String,
        foregroundUserResume: Boolean,
    ): String

    @JvmStatic private external fun nativeRuntimeMarkRegistryTransferSubmitted(
        transferNonce: String,
        osTransferId: String,
    ): String

    @JvmStatic private external fun nativeRuntimeAdoptRegistryTransfer(
        transferNonce: String,
        osTransferId: String,
        observedRequestFingerprint: String,
        observedAndroidJobId: Long,
        matchingTaskCount: Int,
    ): String

    @JvmStatic private external fun nativeRuntimeRecordRegistryTransferMissing(
        transferNonce: String,
        positiveUserStopEvidence: Boolean,
    ): String

    @JvmStatic private external fun nativeRuntimeRegistryTransferScheduleForChannel(
        channelId: String,
    ): String

    @JvmStatic private external fun nativeRuntimePrepareRegistryChunkLedger(
        transferNonce: String,
    ): String

    @JvmStatic private external fun nativeRuntimeRecoverRegistryChunkLedger(
        transferNonce: String,
    ): String

    @JvmStatic private external fun nativeRuntimeRegistryLandingProgress(
        transferNonce: String,
    ): String

    @JvmStatic private external fun nativeRuntimeNextRegistryChunkSourceTarget(
        transferNonce: String,
        artifactRole: Int,
    ): String

    @JvmStatic private external fun nativeRuntimeBeginRegistryChunkWrite(
        transferNonce: String,
        artifactRole: Int,
        chunkIndex: Long,
        sourceOffset: Long,
    ): String

    @JvmStatic private external fun nativeRuntimeAppendRegistryChunkWrite(block: ByteArray): String

    @JvmStatic private external fun nativeRuntimeCheckpointRegistryChunkWrite(): String

    @JvmStatic private external fun nativeRuntimeFinishRegistryChunkWrite(): String

    @JvmStatic private external fun nativeRuntimeSuspendRegistryChunkWrite(): String

    @JvmStatic private external fun nativeRuntimeSaveRawTextDraft(
        contentLanguage: String,
        content: String,
    ): String

    @JvmStatic private external fun nativeRuntimeEnqueueSharedText(
        callbackToken: String,
        mimeType: String,
        content: String,
    ): String

    @JvmStatic private external fun nativeRuntimePendingShareSpoolEntry(index: Int): String

    @JvmStatic private external fun nativeRuntimeImportSharedText(
        spoolRef: String,
        contentLanguage: String,
    ): String

    @JvmStatic private external fun nativeRuntimeStartMediaStage(
        mediaClass: String,
        declaredMimeType: String,
    ): String

    @JvmStatic private external fun nativeRuntimeAppendMediaStage(
        sourceRef: String,
        chunk: ByteArray,
    ): Int

    @JvmStatic private external fun nativeRuntimeFinishMediaStage(sourceRef: String): String

    @JvmStatic private external fun nativeRuntimeFinishOwnedMediaImport(sourceRef: String): String

    @JvmStatic private external fun nativeRuntimeOwnedMediaCount(): Long

    @JvmStatic private external fun nativeRuntimeOwnedMediaEntry(index: Int): String

    @JvmStatic private external fun nativeRuntimeAbortMediaStage(sourceRef: String): Int

    @JvmStatic private external fun nativeRuntimeProcessGeneration(): Long

    @JvmStatic private external fun nativeRuntimeActivationPhase(): Int

    @JvmStatic private external fun nativeRuntimeActiveGrantCount(): Int

    @JvmStatic private external fun nativeRuntimeEncryptedRawDraftCount(): Long

    @JvmStatic private external fun nativeRuntimePendingShareSpoolCount(): Long

    @JvmStatic private external fun nativeRuntimeStagedVerifiedMediaCount(): Long

    @JvmStatic private external fun nativeRuntimeOnboardingCursor(): Int

    @JvmStatic private external fun nativeRuntimeSetOnboardingCursor(cursor: Int): Int

    @JvmStatic private external fun nativeRuntimeRecoveredUncleanStart(): Boolean

    @JvmStatic private external fun nativeRuntimeBootstrapStoreOpened(): Boolean

    @JvmStatic private external fun nativeRuntimeRegistryBootstrapOnly(): Boolean

    @JvmStatic private external fun nativeRuntimeLocalKqlFixtureVerified(): Boolean

    @JvmStatic private external fun nativeRuntimePrivatePlannerVerified(): Boolean

    @JvmStatic private external fun nativeRuntimeNoLlmProvider(): Boolean

    @JvmStatic private external fun nativeRuntimeStaleCallbackRejected(): Boolean

    @JvmStatic private external fun nativeRuntimeSecureProfileActive(): Boolean

    @JvmStatic private external fun nativeRuntimeInstallationBindingVerified(): Boolean

    @JvmStatic private external fun nativeRuntimeInstallationCreated(): Boolean

    @JvmStatic private external fun nativeRuntimeSecuritySessionUnlocked(): Boolean

    @JvmStatic private external fun nativeRuntimePrivateVaultReady(): Boolean

    @JvmStatic private external fun nativeRuntimeIdentityDomainsSeparated(): Boolean

    @JvmStatic private external fun nativeRuntimePrivacyDefaultsFailSafe(): Boolean

    @JvmStatic private external fun nativeRuntimeRedactedHistoryReady(): Boolean
}
