package org.onebrain.onebrain_mobile

import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import android.os.PersistableBundle
import android.util.Log
import androidx.annotation.RequiresApi
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.min

internal enum class RegistryUidtNetworkPolicy(
    val code: Int,
) {
    WIFI_ONLY(0),
    UNMETERED(1),
    ANY(2),
    ;

    companion object {
        fun fromCode(code: Int): RegistryUidtNetworkPolicy =
            entries.firstOrNull { it.code == code }
                ?: error("Unsupported Registry UIDT network policy")
    }
}

internal data class RegistryUidtRequest(
    val transferNonce: String,
    val operationId: String,
    val releaseId: String,
    val manifestDigest: String,
    val trustProfileDigest: String,
    val requestFingerprint: String,
    val transportDescriptorDigest: String,
    val expectedTotalBytes: Long,
    val jobId: Int,
    val networkPolicy: RegistryUidtNetworkPolicy,
    val requiresCharging: Boolean,
) {
    init {
        require(transferNonce.length in 1..128 && transferNonce.all(::isSafeOpaqueChar))
        require(operationId.length in 1..128 && operationId.all(::isSafeOpaqueChar))
        require(releaseId.length == 64 && releaseId.all(::isLowerHex))
        require(manifestDigest.length == 64 && manifestDigest.all(::isLowerHex))
        require(trustProfileDigest.length == 64 && trustProfileDigest.all(::isLowerHex))
        require(requestFingerprint.length == 64 && requestFingerprint.all(::isLowerHex))
        require(
            transportDescriptorDigest.length == 64 &&
                transportDescriptorDigest.all(::isLowerHex),
        )
        require(expectedTotalBytes > 0)
        require(jobId > 0)
    }

    fun matches(schedule: RustRegistryTransferSchedule): Boolean =
        schedule.platformCode == ANDROID_UIDT_PLATFORM_CODE &&
            schedule.androidJobId == jobId &&
            schedule.transferNonce == transferNonce &&
            schedule.operationId == operationId &&
            schedule.releaseId == releaseId &&
            schedule.manifestDigest == manifestDigest &&
            schedule.trustProfileDigest == trustProfileDigest &&
            schedule.requestFingerprint == requestFingerprint &&
            schedule.transportDescriptorDigest == transportDescriptorDigest &&
            schedule.expectedTotalBytes == expectedTotalBytes

    fun toExtras(): PersistableBundle =
        PersistableBundle().apply {
            putInt(EXTRA_CONTRACT_VERSION, CONTRACT_VERSION)
            putString(EXTRA_TRANSFER_NONCE, transferNonce)
            putString(EXTRA_OPERATION_ID, operationId)
            putString(EXTRA_RELEASE_ID, releaseId)
            putString(EXTRA_MANIFEST_DIGEST, manifestDigest)
            putString(EXTRA_TRUST_PROFILE_DIGEST, trustProfileDigest)
            putString(EXTRA_REQUEST_FINGERPRINT, requestFingerprint)
            putString(EXTRA_TRANSPORT_DESCRIPTOR_DIGEST, transportDescriptorDigest)
            putLong(EXTRA_EXPECTED_TOTAL_BYTES, expectedTotalBytes)
            putInt(EXTRA_NETWORK_POLICY, networkPolicy.code)
            putBoolean(EXTRA_REQUIRES_CHARGING, requiresCharging)
        }

    companion object {
        fun fromSchedule(
            schedule: RustRegistryTransferSchedule,
            networkPolicy: RegistryUidtNetworkPolicy,
            requiresCharging: Boolean = true,
        ): RegistryUidtRequest {
            require(schedule.platformCode == ANDROID_UIDT_PLATFORM_CODE)
            return RegistryUidtRequest(
                transferNonce = schedule.transferNonce,
                operationId = schedule.operationId,
                releaseId = schedule.releaseId,
                manifestDigest = schedule.manifestDigest,
                trustProfileDigest = schedule.trustProfileDigest,
                requestFingerprint = schedule.requestFingerprint,
                transportDescriptorDigest = schedule.transportDescriptorDigest,
                expectedTotalBytes = schedule.expectedTotalBytes,
                jobId = checkNotNull(schedule.androidJobId),
                networkPolicy = networkPolicy,
                requiresCharging = requiresCharging,
            )
        }

        fun fromJob(job: JobInfo): RegistryUidtRequest? = fromExtras(job.id, job.extras)

        fun fromExtras(
            jobId: Int,
            extras: PersistableBundle,
        ): RegistryUidtRequest? =
            runCatching {
                require(extras.getInt(EXTRA_CONTRACT_VERSION, -1) == CONTRACT_VERSION)
                RegistryUidtRequest(
                    transferNonce = checkNotNull(extras.getString(EXTRA_TRANSFER_NONCE)),
                    operationId = checkNotNull(extras.getString(EXTRA_OPERATION_ID)),
                    releaseId = checkNotNull(extras.getString(EXTRA_RELEASE_ID)),
                    manifestDigest = checkNotNull(extras.getString(EXTRA_MANIFEST_DIGEST)),
                    trustProfileDigest =
                        checkNotNull(extras.getString(EXTRA_TRUST_PROFILE_DIGEST)),
                    requestFingerprint =
                        checkNotNull(extras.getString(EXTRA_REQUEST_FINGERPRINT)),
                    transportDescriptorDigest =
                        checkNotNull(extras.getString(EXTRA_TRANSPORT_DESCRIPTOR_DIGEST)),
                    expectedTotalBytes = extras.getLong(EXTRA_EXPECTED_TOTAL_BYTES, -1),
                    jobId = jobId,
                    networkPolicy =
                        RegistryUidtNetworkPolicy.fromCode(
                            extras.getInt(EXTRA_NETWORK_POLICY, -1),
                        ),
                    requiresCharging = extras.getBoolean(EXTRA_REQUIRES_CHARGING, true),
                )
            }.getOrNull()
    }
}

internal data class RegistryUidtReconcileResult(
    val status: String,
    val schedule: RustRegistryTransferSchedule?,
    val matchingJobCount: Int,
)

internal object RegistryUidtScheduler {
    const val NAMESPACE = "onebrain.registry.uidt.v1"
    const val OS_TRANSFER_PREFIX = "android-uidt:"

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun scheduleOnly(
        context: Context,
        request: RegistryUidtRequest,
    ): Int {
        val scheduler = scheduler(context)
        check(scheduler.canRunUserInitiatedJobs()) {
            "Android denied RUN_USER_INITIATED_JOBS"
        }
        val jobs = ownJobs(scheduler)
        val sameId = jobs.firstOrNull { it.id == request.jobId }
        check(sameId == null || RegistryUidtRequest.fromJob(sameId) == request) {
            "Prechosen Registry UIDT job ID is already bound to another request"
        }
        if (sameId != null) {
            return sameId.id
        }

        val result = scheduler.schedule(buildJobInfo(context, request))
        check(result == JobScheduler.RESULT_SUCCESS) {
            "Android rejected the visible user-initiated Registry transfer"
        }
        return request.jobId
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun submitAndAdopt(
        context: Context,
        dataRoot: String,
        securityMaterial: ByteArray,
        request: RegistryUidtRequest,
    ): RustRegistryTransferSchedule {
        scheduleOnly(context, request)
        val osTransferId = "$OS_TRANSFER_PREFIX${request.jobId}"
        RustMobileBridge.markRegistryTransferSubmitted(
            dataRoot,
            securityMaterial,
            request.transferNonce,
            osTransferId,
        )
        val matches = matchingJobs(context, request)
        return RustMobileBridge.adoptRegistryTransfer(
            dataRoot = dataRoot,
            securityMaterial = securityMaterial,
            transferNonce = request.transferNonce,
            osTransferId = osTransferId,
            observedRequestFingerprint = request.requestFingerprint,
            observedAndroidJobId = request.jobId,
            matchingTaskCount = matches.size,
        )
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun reconcileChannel(
        context: Context,
        dataRoot: String,
        securityMaterial: ByteArray,
        channelId: String,
    ): RegistryUidtReconcileResult {
        val schedule =
            RustMobileBridge.registryTransferScheduleForChannel(
                dataRoot,
                securityMaterial,
                channelId,
            ) ?: return RegistryUidtReconcileResult("NO_ACTIVE_TRANSFER", null, 0)
        require(schedule.platformCode == ANDROID_UIDT_PLATFORM_CODE)
        val jobId = checkNotNull(schedule.androidJobId)
        val jobs =
            ownJobs(scheduler(context)).mapNotNull { job ->
                RegistryUidtRequest.fromJob(job)?.let { request -> job to request }
            }
        val sameNonce = jobs.filter { (_, request) -> request.transferNonce == schedule.transferNonce }
        val exact = sameNonce.filter { (_, request) -> request.matches(schedule) }
        check(sameNonce.size == exact.size && exact.size <= 1) {
            "Registry UIDT inventory contains a mismatched or duplicate durable request"
        }
        if (exact.size == 1) {
            val request = exact.single().second
            val adopted =
                RustMobileBridge.adoptRegistryTransfer(
                    dataRoot = dataRoot,
                    securityMaterial = securityMaterial,
                    transferNonce = schedule.transferNonce,
                    osTransferId = "$OS_TRANSFER_PREFIX$jobId",
                    observedRequestFingerprint = request.requestFingerprint,
                    observedAndroidJobId = jobId,
                    matchingTaskCount = 1,
                )
            return RegistryUidtReconcileResult("ADOPTED", adopted, 1)
        }
        if (schedule.stateCode == SCHEDULE_PREPARED_STATE_CODE) {
            return RegistryUidtReconcileResult("PREPARED_WITHOUT_JOB", schedule, 0)
        }
        if (schedule.stateCode in TRANSFER_SUBMITTED_STATE_CODE..TRANSFER_ADOPTED_STATE_CODE) {
            val waiting =
                RustMobileBridge.recordRegistryTransferMissing(
                    dataRoot,
                    securityMaterial,
                    schedule.transferNonce,
                    false,
                )
            return RegistryUidtReconcileResult("RESUME_REQUIRED", waiting, 0)
        }
        return RegistryUidtReconcileResult("ALREADY_WAITING", schedule, 0)
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun cancel(
        context: Context,
        jobId: Int,
    ) {
        scheduler(context).cancel(jobId)
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun matchingJobs(
        context: Context,
        request: RegistryUidtRequest,
    ): List<JobInfo> =
        ownJobs(scheduler(context)).filter { job ->
            RegistryUidtRequest.fromJob(job) == request
        }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun requestForJob(
        context: Context,
        jobId: Int,
    ): RegistryUidtRequest? =
        ownJobs(scheduler(context))
            .singleOrNull { it.id == jobId }
            ?.let(RegistryUidtRequest::fromJob)

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun buildJobInfo(
        context: Context,
        request: RegistryUidtRequest,
    ): JobInfo {
        val network =
            NetworkRequest.Builder()
                .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
                .apply {
                    when (request.networkPolicy) {
                        RegistryUidtNetworkPolicy.WIFI_ONLY ->
                            addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                        RegistryUidtNetworkPolicy.UNMETERED ->
                            addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
                        RegistryUidtNetworkPolicy.ANY -> Unit
                    }
                }.build()
        return JobInfo.Builder(
            request.jobId,
            ComponentName(context, RegistryTransferJobService::class.java),
        ).setExtras(request.toExtras())
            .setPersisted(true)
            .setRequiredNetwork(network)
            .setRequiresCharging(request.requiresCharging)
            .setRequiresBatteryNotLow(true)
            .setRequiresStorageNotLow(true)
            .setEstimatedNetworkBytes(request.expectedTotalBytes, 0)
            .setMinimumNetworkChunkBytes(min(request.expectedTotalBytes, REGISTRY_CHUNK_BYTES))
            .setUserInitiated(true)
            .build()
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    private fun scheduler(context: Context): JobScheduler =
        context.getSystemService(JobScheduler::class.java).forNamespace(NAMESPACE)

    private fun ownJobs(scheduler: JobScheduler): List<JobInfo> =
        scheduler.allPendingJobs.filter { job ->
            job.service.className == RegistryTransferJobService::class.java.name
        }
}

internal object RegistryUidtStartupReconciler {
    private val started = AtomicBoolean(false)
    private val executor =
        Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, "onebrain-registry-uidt-reconcile")
        }

    fun reconcileOnce(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE ||
            !started.compareAndSet(false, true)
        ) {
            return
        }
        val appContext = context.applicationContext
        val securityMaterialStore = SecurityMaterialStore(appContext)
        if (!securityMaterialStore.hasExistingInstallationState()) {
            return
        }
        executor.execute {
            var securityMaterial: ByteArray? = null
            try {
                val material = securityMaterialStore.loadOrCreate()
                securityMaterial = material
                val result =
                    RegistryUidtScheduler.reconcileChannel(
                        context = appContext,
                        dataRoot = appContext.noBackupFilesDir.absolutePath,
                        securityMaterial = material,
                        channelId = "stable",
                    )
                Log.i(
                    "OneBrainRegistryUidt",
                    "uidt_startup_reconcile status=${result.status} " +
                        "matches=${result.matchingJobCount}",
                )
            } catch (error: Throwable) {
                Log.w("OneBrainRegistryUidt", "uidt_startup_reconcile_failed", error)
            } finally {
                securityMaterial?.fill(0)
            }
        }
    }
}

private const val CONTRACT_VERSION = 1
private const val ANDROID_UIDT_PLATFORM_CODE = 0
private const val SCHEDULE_PREPARED_STATE_CODE = 0
private const val TRANSFER_SUBMITTED_STATE_CODE = 1
private const val TRANSFER_ADOPTED_STATE_CODE = 2
private const val REGISTRY_CHUNK_BYTES = 8_388_608L

private const val EXTRA_CONTRACT_VERSION = "ob.registry.contract_version"
private const val EXTRA_TRANSFER_NONCE = "ob.registry.transfer_nonce"
private const val EXTRA_OPERATION_ID = "ob.registry.operation_id"
private const val EXTRA_RELEASE_ID = "ob.registry.release_id"
private const val EXTRA_MANIFEST_DIGEST = "ob.registry.manifest_digest"
private const val EXTRA_TRUST_PROFILE_DIGEST = "ob.registry.trust_profile_digest"
private const val EXTRA_REQUEST_FINGERPRINT = "ob.registry.request_fingerprint"
private const val EXTRA_TRANSPORT_DESCRIPTOR_DIGEST = "ob.registry.transport_descriptor_digest"
private const val EXTRA_EXPECTED_TOTAL_BYTES = "ob.registry.expected_total_bytes"
private const val EXTRA_NETWORK_POLICY = "ob.registry.network_policy"
private const val EXTRA_REQUIRES_CHARGING = "ob.registry.requires_charging"

private fun isSafeOpaqueChar(value: Char): Boolean =
    value in 'a'..'z' || value in '0'..'9' || value == '-' || value == '_' || value == ':'

private fun isLowerHex(value: Char): Boolean = value in '0'..'9' || value in 'a'..'f'
