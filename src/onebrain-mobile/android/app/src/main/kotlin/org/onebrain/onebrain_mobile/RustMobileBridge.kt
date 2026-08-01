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

internal object RustMobileBridge {
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
