package com.jyroxa.app.worker

import android.content.Context
import androidx.work.Constraints
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager

object WorkManagerHelper {

    private const val WORK_NAME = "resume_interrupted_downloads"

    /**
     * Enqueues a fallback worker to resume active downloads, honoring Wi-Fi only constraints.
     */
    fun enqueueResumeWork(context: Context, wifiOnly: Boolean) {
        val requiredNetwork = if (wifiOnly) {
            NetworkType.UNMETERED
        } else {
            NetworkType.CONNECTED
        }

        val constraints = Constraints.Builder()
            .setRequiredNetworkType(requiredNetwork)
            .build()

        val request = OneTimeWorkRequestBuilder<ResumeDownloadsWorker>()
            .setConstraints(constraints)
            .build()

        WorkManager.getInstance(context).enqueueUniqueWork(
            WORK_NAME,
            ExistingWorkPolicy.REPLACE,
            request
        )
    }

    /**
     * Cancels pending resume workers if needed.
     */
    fun cancelResumeWork(context: Context) {
        WorkManager.getInstance(context).cancelUniqueWork(WORK_NAME)
    }
}
