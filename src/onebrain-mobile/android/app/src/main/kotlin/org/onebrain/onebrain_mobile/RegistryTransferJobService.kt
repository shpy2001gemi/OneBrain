package org.onebrain.onebrain_mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.job.JobParameters
import android.app.job.JobService
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.PersistableBundle
import android.util.Log
import androidx.annotation.RequiresApi
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.Future

internal class RegistryTransferJobService : JobService() {
    private val executor =
        Executors.newCachedThreadPool { runnable ->
            Thread(runnable, "onebrain-registry-uidt")
        }
    private val running = ConcurrentHashMap<Int, Future<*>>()

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    override fun onStartJob(params: JobParameters): Boolean {
        ensureNotificationChannel()
        val request = RegistryUidtRequest.fromExtras(params.jobId, params.extras)
        if (request == null) {
            Log.e(LOG_TAG, "uidt_rejected reason=INVALID_DURABLE_EXTRAS job=${params.jobId}")
            return false
        }
        setNotification(
            params,
            notificationId(params.jobId),
            notification(request),
            JOB_END_NOTIFICATION_POLICY_REMOVE,
        )
        val task =
            executor.submit {
                val securityMaterialStore = SecurityMaterialStore(applicationContext)
                val securityMaterial = securityMaterialStore.loadOrCreate()
                try {
                    val result =
                        RegistryUidtScheduler.reconcileChannel(
                            context = applicationContext,
                            dataRoot = applicationContext.noBackupFilesDir.absolutePath,
                            securityMaterial = securityMaterial,
                            channelId = REGISTRY_CHANNEL,
                        )
                    Log.i(
                        LOG_TAG,
                        "uidt_started job=${params.jobId} status=${result.status} " +
                            "matches=${result.matchingJobCount}",
                    )
                    // MOB-05B intentionally has no URL/byte executor until an owner-issued
                    // production transport descriptor exists. Finish promptly rather than
                    // pretending that a foreground/UIDT grant is an always-live worker.
                    jobFinished(params, false)
                } catch (error: Throwable) {
                    Log.e(LOG_TAG, "uidt_failed job=${params.jobId}", error)
                    jobFinished(params, false)
                } finally {
                    securityMaterial.fill(0)
                    running.remove(params.jobId)
                }
            }
        running[params.jobId] = task
        return true
    }

    override fun onStopJob(params: JobParameters): Boolean {
        running.remove(params.jobId)?.cancel(true)
        val positiveUserStop =
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
                params.stopReason == JobParameters.STOP_REASON_USER
        recordStoppedJob(params.jobId, params.extras, positiveUserStop)
        return false
    }

    override fun onDestroy() {
        running.values.forEach { it.cancel(true) }
        running.clear()
        executor.shutdownNow()
        super.onDestroy()
    }

    @RequiresApi(Build.VERSION_CODES.O)
    private fun ensureNotificationChannel() {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                NOTIFICATION_CHANNEL_ID,
                getString(R.string.registry_transfer_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = getString(R.string.registry_transfer_channel_description)
                setShowBadge(false)
            },
        )
    }

    @RequiresApi(Build.VERSION_CODES.O)
    private fun notification(request: RegistryUidtRequest): Notification {
        val openApp =
            PendingIntent.getActivity(
                this,
                request.jobId,
                Intent(this, MainActivity::class.java).apply {
                    flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
                },
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        val stop =
            PendingIntent.getBroadcast(
                this,
                request.jobId,
                Intent(this, RegistryTransferControlReceiver::class.java).apply {
                    action = ACTION_STOP_REGISTRY_TRANSFER
                    putExtra(EXTRA_JOB_ID, request.jobId)
                    putExtra(EXTRA_TRANSFER_NONCE, request.transferNonce)
                },
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        return Notification.Builder(this, NOTIFICATION_CHANNEL_ID)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle(getString(R.string.registry_transfer_notification_title))
            .setContentText(
                getString(
                    R.string.registry_transfer_notification_preparing,
                    formatBytes(request.expectedTotalBytes),
                ),
            ).setContentIntent(openApp)
            .setOnlyAlertOnce(true)
            .setOngoing(true)
            .setCategory(Notification.CATEGORY_PROGRESS)
            .addAction(
                Notification.Action.Builder(
                    null,
                    getString(R.string.registry_transfer_notification_stop),
                    stop,
                ).build(),
            ).build()
    }

    private fun recordStoppedJob(
        jobId: Int,
        extras: PersistableBundle,
        positiveUserStop: Boolean,
    ) {
        val request = RegistryUidtRequest.fromExtras(jobId, extras) ?: return
        executor.execute {
            val securityMaterial = SecurityMaterialStore(applicationContext).loadOrCreate()
            try {
                RustMobileBridge.recordRegistryTransferMissing(
                    dataRoot = applicationContext.noBackupFilesDir.absolutePath,
                    securityMaterial = securityMaterial,
                    transferNonce = request.transferNonce,
                    positiveUserStopEvidence = positiveUserStop,
                )
            } catch (error: Throwable) {
                Log.w(LOG_TAG, "uidt_stop_receipt_failed job=$jobId", error)
            } finally {
                securityMaterial.fill(0)
            }
        }
    }

}

internal class RegistryTransferControlReceiver : BroadcastReceiver() {
    override fun onReceive(
        context: Context,
        intent: Intent,
    ) {
        if (intent.action != ACTION_STOP_REGISTRY_TRANSFER ||
            Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE
        ) {
            return
        }
        val jobId = intent.getIntExtra(EXTRA_JOB_ID, -1)
        val transferNonce = intent.getStringExtra(EXTRA_TRANSFER_NONCE) ?: return
        if (jobId <= 0 || transferNonce.length !in 1..128) {
            return
        }
        val pending = goAsync()
        STOP_EXECUTOR.execute {
            val securityMaterial = SecurityMaterialStore(context).loadOrCreate()
            try {
                RustMobileBridge.recordRegistryTransferMissing(
                    dataRoot = context.noBackupFilesDir.absolutePath,
                    securityMaterial = securityMaterial,
                    transferNonce = transferNonce,
                    positiveUserStopEvidence = true,
                )
                RegistryUidtScheduler.cancel(context, jobId)
                Log.i(LOG_TAG, "uidt_user_stop job=$jobId")
            } catch (error: Throwable) {
                Log.e(LOG_TAG, "uidt_user_stop_failed job=$jobId", error)
            } finally {
                securityMaterial.fill(0)
                pending.finish()
            }
        }
    }

    companion object {
        private val STOP_EXECUTOR =
            Executors.newSingleThreadExecutor { runnable ->
                Thread(runnable, "onebrain-registry-uidt-stop")
            }
    }
}

private const val REGISTRY_CHANNEL = "stable"
private const val LOG_TAG = "OneBrainRegistryUidt"
private const val NOTIFICATION_CHANNEL_ID = "onebrain_registry_transfer_v1"
private const val ACTION_STOP_REGISTRY_TRANSFER =
    "org.onebrain.onebrain_mobile.action.STOP_REGISTRY_TRANSFER"
private const val EXTRA_JOB_ID = "org.onebrain.onebrain_mobile.extra.UIDT_JOB_ID"
private const val EXTRA_TRANSFER_NONCE =
    "org.onebrain.onebrain_mobile.extra.UIDT_TRANSFER_NONCE"

private fun notificationId(jobId: Int): Int = 0x2000_0000 or (jobId and 0x0fff_ffff)

private fun formatBytes(bytes: Long): String {
    val gib = bytes.toDouble() / (1024.0 * 1024.0 * 1024.0)
    return "%.2f GiB".format(gib)
}
