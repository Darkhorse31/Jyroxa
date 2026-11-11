package com.swiftget.app.data.model

import kotlinx.serialization.Serializable

/**
 * Represents the progress and range of an individual download segment/thread.
 * Used for multi-connection parallel downloads visualization (like IDM / 1DM / ADM).
 */
@Serializable
data class SegmentProgress(
    val segmentIndex: Int,
    val startByte: Long,
    val endByte: Long,
    val downloadedBytes: Long,
    val speedBps: Double = 0.0,
    val isCompleted: Boolean = false
) {
    val totalBytes: Long
        get() = if (endByte > startByte) endByte - startByte else 0L

    val progressPercent: Float
        get() {
            if (isCompleted) return 100f
            if (totalBytes <= 0) return 0f
            return ((downloadedBytes.toDouble() / totalBytes.toDouble()) * 100.0)
                .toFloat()
                .coerceIn(0f, 100f)
        }
}
