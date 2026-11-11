package com.swiftget.app.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import com.swiftget.app.MainActivity
import com.swiftget.app.R
import com.swiftget.app.SwiftGetApp
import com.swiftget.app.data.model.DownloadItem
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * Foreground service of type "dataSync" running downloads reliably in the background
 * with rich notifications and action buttons (Pause All, Resume All, Cancel).
 */
class DownloadService : Service() {

    private val serviceScope = CoroutineScope(Dispatchers.Main + Job())
    private lateinit var notificationManager: NotificationManager

    companion object {
        const val CHANNEL_ID = "swiftget_downloads_channel"
        const val NOTIFICATION_ID = 1001

        const val ACTION_START = "com.swiftget.app.action.START_SERVICE"
        const val ACTION_PAUSE_ALL = "com.swiftget.app.action.PAUSE_ALL"
        const val ACTION_RESUME_ALL = "com.swiftget.app.action.RESUME_ALL"
        const val ACTION_STOP = "com.swiftget.app.action.STOP_SERVICE"

        fun start(context: Context) {
            val intent = Intent(context, DownloadService::class.java).apply {
                action = ACTION_START
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            val intent = Intent(context, DownloadService::class.java).apply {
                action = ACTION_STOP
            }
            context.startService(intent)
        }
    }

    override fun onCreate() {
        super.onCreate()
        notificationManager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        createNotificationChannel()
        startForegroundInternal(buildInitialNotification())
        observeDownloads()
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                getString(R.string.notification_channel_name),
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = getString(R.string.notification_channel_desc)
                setShowBadge(false)
            }
            notificationManager.createNotificationChannel(channel)
        }
    }

    private fun startForegroundInternal(notification: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun buildInitialNotification(): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.app_name))
            .setContentText("Download engine ready")
            .setSmallIcon(R.drawable.ic_notification)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(createContentIntent())
            .build()
    }

    private fun observeDownloads() {
        val app = application as SwiftGetApp
        val repo = app.downloadRepository

        serviceScope.launch {
            repo.downloads.collect { downloads ->
                val activeDownloads = downloads.filter { it.state.isRunning }
                if (activeDownloads.isEmpty()) {
                    // Stop foreground service when no tasks are running
                    val completed = downloads.any { it.state == com.swiftget.app.data.model.DownloadState.COMPLETED }
                    if (completed) {
                        notificationManager.notify(NOTIFICATION_ID, buildIdleNotification("All downloads completed"))
                    }
                    stopForeground(STOP_FOREGROUND_DETACH)
                    return@collect
                }

                val totalSpeed = activeDownloads.sumOf { it.speedBps }
                val totalBytes = activeDownloads.sumOf { it.totalBytes }
                val downloadedBytes = activeDownloads.sumOf { it.downloadedBytes }
                val overallProgress = if (totalBytes > 0) {
                    ((downloadedBytes.toDouble() / totalBytes) * 100).toInt().coerceIn(0, 100)
                } else 0

                val title = resources.getQuantityString(
                    R.plurals.active_downloads_count,
                    activeDownloads.size,
                    activeDownloads.size
                )
                val text = "${DownloadItem.formatBytesSpeed(totalSpeed)} • ${DownloadItem.formatBytes(downloadedBytes)} / ${DownloadItem.formatBytes(totalBytes)}"

                val notification = NotificationCompat.Builder(this@DownloadService, CHANNEL_ID)
                    .setContentTitle(title)
                    .setContentText(text)
                    .setSmallIcon(R.drawable.ic_notification)
                    .setOngoing(true)
                    .setOnlyAlertOnce(true)
                    .setProgress(100, overallProgress, totalBytes <= 0)
                    .setContentIntent(createContentIntent())
                    .addAction(
                        R.drawable.ic_pause,
                        getString(R.string.notification_action_pause),
                        createActionPendingIntent(ACTION_PAUSE_ALL)
                    )
                    .addAction(
                        R.drawable.ic_play,
                        getString(R.string.notification_action_resume),
                        createActionPendingIntent(ACTION_RESUME_ALL)
                    )
                    .addAction(
                        R.drawable.ic_cancel,
                        getString(R.string.notification_action_cancel),
                        createActionPendingIntent(ACTION_STOP)
                    )
                    .build()

                startForegroundInternal(notification)
            }
        }
    }

    private fun buildIdleNotification(message: String): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(message)
            .setSmallIcon(R.drawable.ic_notification)
            .setAutoCancel(true)
            .setContentIntent(createContentIntent())
            .build()
    }

    private fun createContentIntent(): PendingIntent {
        val intent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        return PendingIntent.getActivity(
            this,
            0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }

    private fun createActionPendingIntent(action: String): PendingIntent {
        val intent = Intent(this, DownloadService::class.java).apply {
            this.action = action
        }
        return PendingIntent.getService(
            this,
            action.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val app = application as SwiftGetApp
        when (intent?.action) {
            ACTION_PAUSE_ALL -> app.downloadRepository.pauseAll()
            ACTION_RESUME_ALL -> app.downloadRepository.resumeAll()
            ACTION_STOP -> {
                app.downloadRepository.pauseAll()
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        super.onDestroy()
        serviceScope.cancel()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
