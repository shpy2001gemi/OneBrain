package org.onebrain.onebrain_mobile

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.StatFs
import android.util.Log
import java.util.concurrent.Executors
import org.json.JSONObject

class RegistryUidtProbeReceiver : BroadcastReceiver() {
    override fun onReceive(
        context: Context,
        intent: Intent,
    ) {
        if (intent.action != ACTION_PROBE) {
            return
        }
        val mode = intent.getStringExtra(EXTRA_MODE) ?: MODE_SCHEDULE_ONLY
        val pending = goAsync()
        EXECUTOR.execute {
            try {
                require(Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                    "Android 14 is required for UIDT"
                }
                val result =
                    when (mode) {
                        MODE_SCHEDULE_ONLY -> scheduleOnly(context)
                        MODE_RECONCILE -> reconcile(context)
                        MODE_STOP -> stop(context)
                        else -> error("Unsupported UIDT probe mode")
                    }
                Log.i(LOG_TAG, result.toString())
            } catch (error: Throwable) {
                Log.e(
                    LOG_TAG,
                    JSONObject()
                        .put("status", "FAILED")
                        .put("mode", mode)
                        .put("error", error.javaClass.simpleName)
                        .put("message", error.message ?: "unknown")
                        .toString(),
                    error,
                )
            } finally {
                pending.finish()
            }
        }
    }

    private fun scheduleOnly(context: Context): JSONObject =
        withSecurityMaterial(context) { dataRoot, securityMaterial ->
            val fixture = loadFixture(context)
            val storage = StatFs(dataRoot)
            val plan =
                RustMobileBridge.prepareRegistryInit(
                    dataRoot = dataRoot,
                    securityMaterial = securityMaterial,
                    channelId = REGISTRY_CHANNEL,
                    trustProfile = fixture.trustProfile,
                    channelHead = fixture.channelHead,
                    release = fixture.release,
                    allocationUnitBytes = storage.blockSizeLong,
                    destinationTotalUsableBytes = storage.totalBytes,
                    measuredFreeBytes = storage.availableBytes,
                )
            val confirmed =
                RustMobileBridge.confirmRegistryInit(
                    dataRoot = dataRoot,
                    securityMaterial = securityMaterial,
                    operationId = plan.operationId,
                    manifestDigest = plan.manifestDigest,
                    trustProfile = fixture.trustProfile,
                    networkPolicyCode = RegistryUidtNetworkPolicy.UNMETERED.code,
                    oneTimeNetworkOverride = false,
                    allocationUnitBytes = storage.blockSizeLong,
                    destinationTotalUsableBytes = storage.totalBytes,
                    measuredFreeBytes = storage.availableBytes,
                )
            val schedule =
                RustMobileBridge.prepareRegistryTransferSchedule(
                    dataRoot = dataRoot,
                    securityMaterial = securityMaterial,
                    operationId = confirmed.operationId,
                    manifestDigest = confirmed.manifestDigest,
                    platformCode = 0,
                    requestFingerprint = REQUEST_FINGERPRINT,
                    transportDescriptorDigest = DEVELOPMENT_DESCRIPTOR_DIGEST,
                    expectedTotalBytes = confirmed.artifactTotalBytes,
                    foregroundUserResume = false,
                )
            val request =
                RegistryUidtRequest.fromSchedule(
                    schedule,
                    RegistryUidtNetworkPolicy.UNMETERED,
                    requiresCharging = true,
                )
            RegistryUidtScheduler.scheduleOnly(context, request)
            JSONObject()
                .put("status", "SCHEDULED_ONLY")
                .put("job_id", request.jobId)
                .put("transfer_nonce", request.transferNonce)
                .put("request_fingerprint", request.requestFingerprint)
                .put("expected_total_bytes", request.expectedTotalBytes)
                .put("rust_state", schedule.stateCode)
        }

    private fun reconcile(context: Context): JSONObject =
        withSecurityMaterial(context) { dataRoot, securityMaterial ->
            val result =
                RegistryUidtScheduler.reconcileChannel(
                    context = context,
                    dataRoot = dataRoot,
                    securityMaterial = securityMaterial,
                    channelId = REGISTRY_CHANNEL,
                )
            JSONObject()
                .put("status", result.status)
                .put("matching_job_count", result.matchingJobCount)
                .put("rust_state", result.schedule?.stateCode ?: -1)
                .put("job_id", result.schedule?.androidJobId ?: -1)
                .put("transfer_nonce", result.schedule?.transferNonce ?: "")
        }

    private fun stop(context: Context): JSONObject =
        withSecurityMaterial(context) { dataRoot, securityMaterial ->
            val schedule =
                checkNotNull(
                    RustMobileBridge.registryTransferScheduleForChannel(
                        dataRoot,
                        securityMaterial,
                        REGISTRY_CHANNEL,
                    ),
                )
            val waiting =
                RustMobileBridge.recordRegistryTransferMissing(
                    dataRoot = dataRoot,
                    securityMaterial = securityMaterial,
                    transferNonce = schedule.transferNonce,
                    positiveUserStopEvidence = true,
                )
            RegistryUidtScheduler.cancel(context, checkNotNull(schedule.androidJobId))
            JSONObject()
                .put("status", "USER_STOPPED")
                .put("rust_state", waiting.stateCode)
                .put("job_id", schedule.androidJobId)
        }

    private fun <T> withSecurityMaterial(
        context: Context,
        block: (String, ByteArray) -> T,
    ): T {
        val securityMaterial = SecurityMaterialStore(context).loadOrCreate()
        return try {
            block(context.noBackupFilesDir.absolutePath, securityMaterial)
        } finally {
            securityMaterial.fill(0)
        }
    }

    private fun loadFixture(context: Context): DevelopmentFixture =
        DevelopmentFixture(
            trustProfile = readHexAsset(context, "mob05a/registry_trust_profile.cbor.hex"),
            channelHead = readHexAsset(context, "mob05a/registry_channel_head.cbor.hex"),
            release = readHexAsset(context, "mob05a/registry_release.cbor.hex"),
        )

    private fun readHexAsset(
        context: Context,
        name: String,
    ): ByteArray {
        val value = context.assets.open(name).bufferedReader().use { it.readText() }.trim()
        require(value.isNotEmpty() && value.length % 2 == 0 && value.all(::isHex))
        return ByteArray(value.length / 2) { index ->
            value.substring(index * 2, index * 2 + 2).toInt(16).toByte()
        }
    }

    companion object {
        private val EXECUTOR =
            Executors.newSingleThreadExecutor { runnable ->
                Thread(runnable, "onebrain-registry-uidt-probe")
            }
    }
}

private data class DevelopmentFixture(
    val trustProfile: ByteArray,
    val channelHead: ByteArray,
    val release: ByteArray,
)

private const val ACTION_PROBE =
    "org.onebrain.onebrain_mobile.debug.REGISTRY_UIDT_PROBE"
private const val EXTRA_MODE = "mode"
private const val MODE_SCHEDULE_ONLY = "schedule_only"
private const val MODE_RECONCILE = "reconcile"
private const val MODE_STOP = "stop"
private const val REGISTRY_CHANNEL = "stable"
private const val LOG_TAG = "OneBrainRegistryUidtProbe"
private const val REQUEST_FINGERPRINT =
    "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd"
private const val DEVELOPMENT_DESCRIPTOR_DIGEST =
    "efefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef"

private fun isHex(value: Char): Boolean =
    value in '0'..'9' || value.lowercaseChar() in 'a'..'f'
