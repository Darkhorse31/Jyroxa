package com.jyroxa.app.data.model

import kotlinx.serialization.Serializable

/**
 * Historical speed point for real-time speed graph visualization.
 */
@Serializable
data class SpeedSample(
    val timestampMs: Long,
    val speedBps: Double
) {
    val speedKbps: Double
        get() = speedBps / 1024.0

    val speedMbps: Double
        get() = speedBps / (1024.0 * 1024.0)
}
