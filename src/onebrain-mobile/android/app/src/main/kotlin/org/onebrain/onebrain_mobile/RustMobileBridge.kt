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
    val onboardingCursor: Int,
)

internal data class RustRawDraftReceipt(
    val draftRef: String,
    val contentLanguage: String,
    val contentBytes: Long,
    val totalDrafts: Long,
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
            profileVersion = "MOB-04/1",
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

    @JvmStatic private external fun nativeRuntimeProcessGeneration(): Long

    @JvmStatic private external fun nativeRuntimeActivationPhase(): Int

    @JvmStatic private external fun nativeRuntimeActiveGrantCount(): Int

    @JvmStatic private external fun nativeRuntimeEncryptedRawDraftCount(): Long

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
