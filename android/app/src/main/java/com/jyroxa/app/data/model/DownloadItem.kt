package com.jyroxa.app.data.model

import kotlinx.serialization.Serializable
import java.util.Locale

@Serializable
data class DownloadItem(
    val id: Long,
    val url: String,
    val outputFile: String,
    val fileName: String,
    val state: DownloadState = DownloadState.QUEUED,
    val downloadedBytes: Long = 0L,
    val totalBytes: Long = 0L,
    val speedBps: Double = 0.0,
    val averageSpeedBps: Double = 0.0,
    val progressPercent: Float = 0f,
    val connections: Int = 8,
    val etaSeconds: Long? = null,
    val segments: List<SegmentProgress> = emptyList(),
    val speedHistory: List<SpeedSample> = emptyList(),
    val errorMessage: String? = null,
    val createdAt: Long = System.currentTimeMillis(),
    val completedAt: Long? = null
) {
    val formattedSpeed: String
        get() = formatBytesSpeed(speedBps)

    val formattedAverageSpeed: String
        get() = formatBytesSpeed(averageSpeedBps)

    val formattedSpeedAndAvg: String
        get() = if (averageSpeedBps > 0.0 && state.isRunning) {
            "${formatBytesSpeed(speedBps)} (avg ${formatBytesSpeed(averageSpeedBps)})"
        } else {
            formatBytesSpeed(speedBps)
        }

    val formattedDownloaded: String
        get() = formatBytes(downloadedBytes)

    val formattedTotal: String
        get() = if (totalBytes > 0) formatBytes(totalBytes) else "Unknown"

    val formattedEta: String
        get() = when {
            state == DownloadState.COMPLETED -> "Done"
            state == DownloadState.PAUSED -> "Paused"
            state == DownloadState.FAILED -> "Error"
            etaSeconds == null || etaSeconds <= 0 -> "--:--"
            etaSeconds < 60 -> "${etaSeconds}s left"
            etaSeconds < 3600 -> String.format(Locale.US, "%dm %02ds left", etaSeconds / 60, etaSeconds % 60)
            else -> String.format(Locale.US, "%dh %02dm left", etaSeconds / 3600, (etaSeconds % 3600) / 60)
        }

    val fileCategory: LinkCategory
        get() {
            val ext = fileName.substringAfterLast('.', "")
            return GrabbedLink.detectCategory(ext)
        }

    val isIndeterminate: Boolean
        get() = totalBytes <= 0 && state.isRunning

    companion object {
        fun formatBytes(bytes: Long): String {
            if (bytes <= 0) return "0 B"
            val kb = bytes / 1024.0
            val mb = kb / 1024.0
            val gb = mb / 1024.0
            return when {
                gb >= 1.0 -> String.format(Locale.US, "%.2f GB", gb)
                mb >= 1.0 -> String.format(Locale.US, "%.2f MB", mb)
                kb >= 1.0 -> String.format(Locale.US, "%.1f KB", kb)
                else -> "$bytes B"
            }
        }

        fun formatBytesSpeed(speedBps: Double): String {
            if (speedBps <= 0.0) return "0 B/s"
            val kb = speedBps / 1024.0
            val mb = kb / 1024.0
            val gb = mb / 1024.0
            return when {
                gb >= 1.0 -> String.format(Locale.US, "%.2f GB/s", gb)
                mb >= 1.0 -> String.format(Locale.US, "%.2f MB/s", mb)
                kb >= 1.0 -> String.format(Locale.US, "%.1f KB/s", kb)
                else -> String.format(Locale.US, "%.0f B/s", speedBps)
            }
        }
    }
}
