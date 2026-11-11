package com.swiftget.app.data.model

import androidx.compose.ui.graphics.Color
import kotlinx.serialization.Serializable

@Serializable
enum class DownloadState(val label: String) {
    QUEUED("Queued"),
    STARTING("Connecting"),
    RUNNING("Downloading"),
    PAUSED("Paused"),
    COMPLETED("Completed"),
    FAILED("Failed"),
    CANCELLED("Cancelled");

    val isTerminal: Boolean
        get() = this == COMPLETED || this == FAILED || this == CANCELLED

    val isRunning: Boolean
        get() = this == STARTING || this == RUNNING

    val isPaused: Boolean
        get() = this == PAUSED

    companion object {
        fun fromNative(nativeState: String): DownloadState {
            val lower = nativeState.lowercase().trim()
            return when {
                lower == "starting" || lower == "connecting" -> STARTING
                lower == "running" || lower == "downloading" -> RUNNING
                lower == "paused" -> PAUSED
                lower == "completed" -> COMPLETED
                lower == "cancelled" || lower == "canceled" -> CANCELLED
                lower.startsWith("failed") || lower.startsWith("error") -> FAILED
                else -> QUEUED
            }
        }
    }
}
