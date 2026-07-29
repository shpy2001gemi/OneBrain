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

    @JvmStatic private external fun nativeAbiVersion(): Int

    @JvmStatic private external fun nativeCoreVersion(): String

    @JvmStatic private external fun nativeRegistryRequestIssued(): Boolean

    @JvmStatic private external fun nativeRoundTrip(nonce: Long): Long
}
