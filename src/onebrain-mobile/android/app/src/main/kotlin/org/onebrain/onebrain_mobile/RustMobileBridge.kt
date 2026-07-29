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

    fun inspectRuntime(dataRoot: String): RustRuntimeFacts {
        check(loadFailure == null) {
            "Rust mobile bridge is unavailable: ${loadFailure?.message}"
        }
        val statusCode = nativeRuntimeOpen(dataRoot)
        check(statusCode == 0) {
            "Rust mobile runtime failed to open with status $statusCode"
        }
        return RustRuntimeFacts(
            profileVersion = "MOB-02/1",
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
        )
    }

    @JvmStatic private external fun nativeAbiVersion(): Int

    @JvmStatic private external fun nativeCoreVersion(): String

    @JvmStatic private external fun nativeRegistryRequestIssued(): Boolean

    @JvmStatic private external fun nativeRoundTrip(nonce: Long): Long

    @JvmStatic private external fun nativeRuntimeOpen(dataRoot: String): Int

    @JvmStatic private external fun nativeRuntimeProcessGeneration(): Long

    @JvmStatic private external fun nativeRuntimeActivationPhase(): Int

    @JvmStatic private external fun nativeRuntimeActiveGrantCount(): Int

    @JvmStatic private external fun nativeRuntimeRecoveredUncleanStart(): Boolean

    @JvmStatic private external fun nativeRuntimeBootstrapStoreOpened(): Boolean

    @JvmStatic private external fun nativeRuntimeRegistryBootstrapOnly(): Boolean

    @JvmStatic private external fun nativeRuntimeLocalKqlFixtureVerified(): Boolean

    @JvmStatic private external fun nativeRuntimePrivatePlannerVerified(): Boolean

    @JvmStatic private external fun nativeRuntimeNoLlmProvider(): Boolean

    @JvmStatic private external fun nativeRuntimeStaleCallbackRejected(): Boolean
}
