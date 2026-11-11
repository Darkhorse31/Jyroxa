package com.jyroxa.app.worker

import android.content.Context
import android.util.Log
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import com.jyroxa.app.JyroxaApp
import com.jyroxa.app.data.model.DownloadState
import com.jyroxa.app.service.DownloadService

/**
 * Fallback worker triggered by WorkManager (after reboot or process termination)
 * to resume active or interrupted downloads when network constraints are satisfied.
 */
class ResumeDownloadsWorker(
    appContext: Context,
    workerParams: WorkerParameters
) : CoroutineWorker(appContext, workerParams) {

    companion object {
        private const val TAG = "ResumeDownloadsWorker"
    }

    override suspend fun doWork(): Result {
        Log.i(TAG, "Executing ResumeDownloadsWorker fallback check")
        val app = applicationContext as? JyroxaApp ?: return Result.failure()
        val repo = app.downloadRepository
        val settings = app.settings.value

        // If Wi-Fi only is enforced and network is currently metered, retry when unmetered
        if (settings.wifiOnly && !app.networkMonitor.checkIsUnmetered()) {
            Log.i(TAG, "Wi-Fi only constraint active and network is metered. Postponing resume.")
            return Result.retry()
        }

        // Find downloads that were running or starting when the process stopped
        val interrupted = repo.downloads.value.filter {
            it.state == DownloadState.RUNNING || it.state == DownloadState.STARTING
        }

        if (interrupted.isNotEmpty()) {
            Log.i(TAG, "Resuming ${interrupted.size} interrupted downloads")
            DownloadService.start(applicationContext)
            interrupted.forEach { item ->
                repo.resumeDownload(item.id)
            }
        } else {
            Log.i(TAG, "No interrupted downloads found to resume")
        }

        return Result.success()
    }
}
